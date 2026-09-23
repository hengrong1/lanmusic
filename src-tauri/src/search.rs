//! 搜索：多字段匹配（标题/艺人/专辑/歌词/文件名）+ 拼音 + 相关度排序。
//!
//! 流程：SQL 预筛候选行（视图过滤 + 启用的非歌词字段 LIKE 粗筛）→
//! Rust 侧逐字段精确匹配（含拼音）→ 相关度评分 → 内存排序分页。
//! 歌词命中依赖 lyrics_index 表（扫描时写入；存量库启动时后台回填）。

use std::collections::HashMap;

use pinyin::ToPinyin;
use rusqlite::{params_from_iter, Connection};

use crate::commands::{self, attach_artists, Page, Track, TrackQuery};
use crate::state::AppState;

const F_TITLE: &str = "title";
const F_ARTIST: &str = "artist";
const F_ALBUM: &str = "album";
const F_LYRICS: &str = "lyrics";
const F_FILENAME: &str = "filename";

const VALID_FIELDS: &[&str] = &[F_TITLE, F_ARTIST, F_ALBUM, F_LYRICS, F_FILENAME];

/// 字段权重（相关度评分）
fn field_weight(f: &str) -> i32 {
    match f {
        F_TITLE => 100,
        F_ARTIST => 80,
        F_ALBUM => 60,
        F_FILENAME => 40,
        F_LYRICS => 20,
        _ => 0,
    }
}

fn default_fields() -> Vec<String> {
    vec![
        F_TITLE.to_string(),
        F_ARTIST.to_string(),
        F_ALBUM.to_string(),
    ]
}

/// 解析启用的搜索字段（非法值过滤；空则回退默认 标题/艺人/专辑）
fn enabled_fields(q: &TrackQuery) -> Vec<String> {
    match &q.fields {
        Some(list) if !list.is_empty() => {
            let f: Vec<String> = list
                .iter()
                .filter(|x| VALID_FIELDS.contains(&x.as_str()))
                .cloned()
                .collect();
            if f.is_empty() {
                default_fields()
            } else {
                f
            }
        }
        _ => default_fields(),
    }
}

/// SQL LIKE 通配符转义（query_folders 等按路径前缀 LIKE 的查询也要用它）
pub(crate) fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// 拼音形式：(全拼, 首字母)。忽略空白；非汉字字符小写原样保留（便于中英混合匹配）。
fn pinyin_forms(s: &str) -> (String, String) {
    let mut full = String::new();
    let mut initials = String::new();
    for c in s.chars() {
        if c.is_whitespace() {
            continue;
        }
        if let Some(py) = c.to_pinyin() {
            let p = py.plain();
            full.push_str(p);
            initials.push(p.chars().next().unwrap_or(c));
        } else {
            let lc = c.to_ascii_lowercase();
            full.push(lc);
            initials.push(lc);
        }
    }
    (full, initials)
}

/// 单字段匹配分数（0 = 未命中）。命中加分：直接包含 + 前缀 + 全等；拼音侧重 60%。
fn match_field(
    field: &str,
    text: &str,
    q_lower: &str,
    q_full: &str,
    q_init: &str,
    pinyin_active: bool,
    cache: &mut HashMap<String, (String, String)>,
) -> i32 {
    let lower = text.to_lowercase();
    let weight = field_weight(field);
    let mut score = 0;
    if lower.contains(q_lower) {
        score += weight;
        if lower == q_lower {
            score += 40;
        } else if lower.starts_with(q_lower) {
            score += 20;
        }
    }
    // 拼音匹配仅对非歌词字段（歌词多为口语/外语，转拼音噪音大）
    if pinyin_active && field != F_LYRICS {
        let (full, init) = if let Some(v) = cache.get(text) {
            v.clone()
        } else {
            let v = pinyin_forms(text);
            cache.insert(text.to_string(), v.clone());
            v
        };
        if !q_full.is_empty() && full.contains(q_full) {
            score += (weight as f32 * 0.6) as i32;
        }
        if !q_init.is_empty() && init.contains(q_init) {
            score += (weight as f32 * 0.4) as i32;
        }
    }
    score
}

/// 候选行：Track + 排序/评分辅助字段 + 歌词索引文本
struct Hit {
    track: Track,
    score: i32,
    added_at: i64,
    play_count: i64,
    lyrics: String,
}

fn row_hit(r: &rusqlite::Row) -> rusqlite::Result<Hit> {
    Ok(Hit {
        track: Track {
            id: r.get(0)?,
            title: r.get(1)?,
            artist: r.get(2)?,
            artist_id: r.get(3)?,
            album: r.get(4)?,
            album_id: r.get(5)?,
            track_no: r.get(6)?,
            disc_no: r.get(7)?,
            duration: r.get(8)?,
            bitrate: r.get(9)?,
            sample_rate: r.get(10)?,
            bit_depth: r.get(11)?,
            format: r.get(12)?,
            path: r.get(13)?,
            has_lyrics: r.get::<_, i64>(14)? != 0,
            has_mv: r.get::<_, i64>(15)? != 0,
            fav: r.get::<_, i64>(16)? != 0,
            artists: Vec::new(),
            matched_fields: Vec::new(),
            rg_track_gain: r.get(20)?,
            rg_track_peak: r.get(21)?,
        },
        score: 0,
        added_at: r.get(17)?,
        play_count: r.get(18)?,
        lyrics: r.get(19)?,
    })
}
/// 搜索曲目：多字段匹配 + 拼音 + 相关度排序，内存分页。
pub fn search_tracks(conn: &Connection, q: &TrackQuery) -> Result<Page<Track>, String> {
    let raw = q.search.as_deref().unwrap_or("").trim();
    if raw.is_empty() {
        return Ok(Page {
            total: 0,
            items: Vec::new(),
        });
    }
    let fields = enabled_fields(q);
    let pinyin_on = q.pinyin.unwrap_or(true);
    // 拼音仅在查询为纯 ASCII 时启用（中文查询直接按原文匹配即可，避免无谓的转换开销）
    let pinyin_active = pinyin_on && raw.chars().all(|c| c.is_ascii());
    let lyrics_enabled = fields.iter().any(|f| f == F_LYRICS);
    let sort_pref = q.sort.as_deref().unwrap_or("relevance");

    let q_lower = raw.to_lowercase();
    let q_compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    let (q_full, q_init) = pinyin_forms(&q_compact);

    // ---- SQL 候选集：视图过滤 + 可预筛字段 LIKE ----
    let (view_where, view_args) = commands::view_filter(q);
    let mut wheres: Vec<String> = Vec::new();
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if !view_where.is_empty() {
        wheres.push(view_where);
    }
    args.extend(view_args);

    // 拼音开启或歌词开启时无法安全地按原文 LIKE 预筛（会漏掉拼音/纯歌词命中），走全量候选
    let can_prefilter = !pinyin_active && !lyrics_enabled;
    if can_prefilter {
        let like = format!("%{}%", like_escape(raw));
        let mut ors: Vec<String> = Vec::new();
        for f in &fields {
            match f.as_str() {
                F_TITLE => {
                    ors.push("t.title LIKE ? ESCAPE '\\'".into());
                    args.push(Box::new(like.clone()));
                }
                F_ALBUM => {
                    ors.push("IFNULL(al.title,'') LIKE ? ESCAPE '\\'".into());
                    args.push(Box::new(like.clone()));
                }
                F_FILENAME => {
                    ors.push("t.path LIKE ? ESCAPE '\\'".into());
                    args.push(Box::new(like.clone()));
                }
                F_ARTIST => {
                    // 主艺人 + track_artists 合作艺人都命中
                    ors.push("(IFNULL(a.name,'') LIKE ? ESCAPE '\\' OR EXISTS (SELECT 1 FROM track_artists ta JOIN artists a2 ON a2.id = ta.artist_id WHERE ta.track_id = t.id AND a2.name LIKE ? ESCAPE '\\'))".into());
                    args.push(Box::new(like.clone()));
                    args.push(Box::new(like.clone()));
                }
                _ => {}
            }
        }
        if !ors.is_empty() {
            wheres.push(format!("({})", ors.join(" OR ")));
        }
    }
    let where_sql = if wheres.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", wheres.join(" AND "))
    };
    let sql = format!(
        "SELECT t.id, t.title, a.name, t.artist_id, al.title, t.album_id, t.track_no, t.disc_no, \
                t.duration, t.bitrate, t.sample_rate, t.bit_depth, t.format, t.path, t.has_embedded_lyrics, t.has_mv, t.fav, \
                t.added_at, t.play_count, IFNULL(li.text,''), t.rg_track_gain, t.rg_track_peak \
         FROM tracks t \
         LEFT JOIN artists a ON a.id = t.artist_id \
         LEFT JOIN albums al ON al.id = t.album_id \
         LEFT JOIN lyrics_index li ON li.track_id = t.id \
         {where_sql} \
         ORDER BY t.id"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params_from_iter(args.iter().map(|b| b.as_ref())), row_hit)
        .map_err(|e| e.to_string())?;
    let mut hits: Vec<Hit> = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    // ----- 艺人列表（含合作艺人）用于匹配与展示 -----
    let mut tracks: Vec<Track> = hits.iter().map(|h| h.track.clone()).collect();
    attach_artists(conn, &mut tracks)?;
    for (h, t) in hits.iter_mut().zip(tracks) {
        h.track = t;
    }

    // ----- 逐字段精确匹配 + 评分 -----
    let mut cache: HashMap<String, (String, String)> = HashMap::new();
    for h in hits.iter_mut() {
        let mut score = 0i32;
        let mut matched: Vec<String> = Vec::new();
        for f in &fields {
            let texts: Vec<&str> = match f.as_str() {
                F_TITLE => vec![h.track.title.as_str()],
                F_ARTIST => {
                    if !h.track.artists.is_empty() {
                        h.track.artists.iter().map(|a| a.name.as_str()).collect()
                    } else {
                        h.track
                            .artist
                            .as_deref()
                            .map(|s| vec![s])
                            .unwrap_or_default()
                    }
                }
                F_ALBUM => h
                    .track
                    .album
                    .as_deref()
                    .map(|s| vec![s])
                    .unwrap_or_default(),
                F_FILENAME => vec![h.track.path.as_str()],
                F_LYRICS => vec![h.lyrics.as_str()],
                _ => Vec::new(),
            };
            let mut best = 0;
            for text in texts {
                let s = match_field(
                    f,
                    text,
                    &q_lower,
                    &q_full,
                    &q_init,
                    pinyin_active,
                    &mut cache,
                );
                if s > best {
                    best = s;
                }
            }
            if best > 0 {
                score += best;
                matched.push(f.clone());
            }
        }
        h.score = score;
        h.track.matched_fields = matched;
    }
    hits.retain(|h| h.score > 0);

    // 按 track_id 去重：保留最高分的一条（防御性，防止 JOIN 意外产生重复行）
    {
        let mut best_by_id: HashMap<i64, usize> = HashMap::new();
        for (i, h) in hits.iter().enumerate() {
            best_by_id
                .entry(h.track.id)
                .and_modify(|e| {
                    if h.score > hits[*e].score {
                        *e = i;
                    }
                })
                .or_insert(i);
        }
        let mut idxs: Vec<usize> = best_by_id.into_values().collect();
        idxs.sort_unstable();
        let mut deduped: Vec<Hit> = Vec::with_capacity(idxs.len());
        for i in idxs.into_iter().rev() {
            deduped.push(hits.swap_remove(i));
        }
        deduped.reverse();
        hits = deduped;
    }

    // ----- 排序偏好 -----
    match sort_pref {
        "added" => hits.sort_by(|a, b| {
            b.added_at
                .cmp(&a.added_at)
                .then(b.score.cmp(&a.score))
                .then(a.track.id.cmp(&b.track.id))
        }),
        "plays" => hits.sort_by(|a, b| {
            b.play_count
                .cmp(&a.play_count)
                .then(b.score.cmp(&a.score))
                .then(a.track.id.cmp(&b.track.id))
        }),
        _ => hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.track.id.cmp(&b.track.id))),
    }

    // ----- 分页 -----
    let page = q.page.unwrap_or(0) as usize;
    let page_size = q.page_size.unwrap_or(200).clamp(1, 5000) as usize;
    let total = hits.len() as i64;
    let items: Vec<Track> = hits
        .into_iter()
        .skip(page.saturating_mul(page_size))
        .take(page_size)
        .map(|h| h.track)
        .collect();

    Ok(Page { total, items })
}

/// 启动时后台回填歌词搜索索引：处理最多 BATCH 首尚无索引的曲目（跨启动续跑，不阻塞 UI）。
pub fn backfill_lyrics_index(app: &tauri::AppHandle) {
    use rusqlite::params;
    use tauri::Manager;

    const BATCH: i64 = 2000;
    let ids: Vec<i64> = {
        let state = app.state::<AppState>();
        let Ok(conn) = state.db.lock() else { return };
        let Ok(mut stmt) = conn.prepare(
            "SELECT t.id FROM tracks t \
             LEFT JOIN lyrics_index li ON li.track_id = t.id \
             WHERE li.track_id IS NULL \
               AND (t.has_embedded_lyrics = 1 OR EXISTS (SELECT 1 FROM lrc_files l WHERE l.track_id = t.id)) \
             ORDER BY t.id LIMIT ?1",
        ) else {
            return;
        };
        let Ok(rows) = stmt.query_map([BATCH], |r| r.get::<_, i64>(0)) else {
            return;
        };
        rows.filter_map(|r| r.ok()).collect()
    };

    for id in ids {
        if let Ok(Some(text)) = crate::lyrics::fetch(app, id) {
            if !text.trim().is_empty() {
                let state = app.state::<AppState>();
                let lock = state.db.lock();
                if let Ok(conn) = lock {
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO lyrics_index (track_id, text) VALUES (?1, ?2)",
                        params![id, text],
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_like_wildcards() {
        assert_eq!(like_escape("50%_off"), "50\\%\\_off");
        assert_eq!(like_escape("a\\b"), "a\\\\b");
    }

    #[test]
    fn pinyin_forms_work() {
        assert_eq!(pinyin_forms("周杰伦").0, "zhoujielun");
        assert_eq!(pinyin_forms("周杰伦").1, "zjl");
        // 中英混合：非汉字小写原样保留，空白忽略
        assert_eq!(pinyin_forms("爱 love 你").0, "ailoveni");
        assert_eq!(pinyin_forms("Jay Chou").0, "jaychou");
    }

    #[test]
    fn matches_direct_and_pinyin() {
        let mut cache = HashMap::new();
        // 中文直接命中
        let s = match_field("title", "晴天", "晴天", "qingtian", "qt", false, &mut cache);
        assert!(s > 0);
        // 拼音全拼命中
        let s = match_field(
            "title", "晴天", "qingtian", "qingtian", "qt", true, &mut cache,
        );
        assert!(s > 0);
        // 拼音首字母命中
        let s = match_field("title", "晴天", "qt", "qt", "qt", true, &mut cache);
        assert!(s > 0);
        // 不相关不命中
        let s = match_field("title", "蓝天", "晴天", "qingtian", "qt", true, &mut cache);
        assert_eq!(s, 0);
    }
}
