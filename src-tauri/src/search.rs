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
            matched_lyric: None,
            matched_artist_ids: Vec::new(),
            rg_track_gain: r.get(20)?,
            rg_track_peak: r.get(21)?,
        },
        score: 0,
        added_at: r.get(17)?,
        play_count: r.get(18)?,
        lyrics: r.get(19)?,
    })
}

/// 歌词行展示上限（字符）：超长行截断加省略号，避免结果行被一行歌词撑爆
const LYRIC_SNIPPET_MAX: usize = 80;

/// `[` 后的内容是否为 LRC 时间戳（`mm:ss` / `mm:ss.xx`）：冒号两侧都是数字。
/// 借此区分时间戳与元信息标签（`[ar:周杰伦]` / `[ti:...]` / `[offset:...]`）。
fn is_time_tag(s: &str) -> bool {
    let Some((mm, ss)) = s.split_once(':') else {
        return false;
    };
    !mm.is_empty()
        && mm.chars().all(|c| c.is_ascii_digit())
        && !ss.is_empty()
        && ss.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// 删除字符串里所有 `<x>...</x>` 式包裹段（A2 增强 LRC 的行内字级时间标签 `<mm:ss.xx>`）
fn remove_angle_segments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        match rest[open + 1..].find('>') {
            Some(rel) => {
                rest = &rest[open + 1 + rel + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 剥掉歌词行的标记，只留可读文本：行内 A2 字标签 `<mm:ss.xx>` 与时间戳
/// `[mm:ss.xx]`（可多个，如 `[00:12][01:20]同一句`）删除；元信息行
/// （`[ar:...]` / `[ti:...]` / `[offset:...]` 等非时间戳方括号开头）返回空串跳过。
fn strip_lyric_tags(line: &str) -> String {
    if line.starts_with('[') {
        if let Some(end) = line.find(']') {
            if !is_time_tag(&line[1..end]) {
                return String::new();
            }
        }
    }
    let s = remove_angle_segments(line);
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(i) = rest.find('[') {
        match rest[i..].find(']') {
            Some(rel) => {
                let close = i + rel;
                // 时间戳删除；非时间戳方括号（歌词里手写的注释）原样保留
                if is_time_tag(&rest[i + 1..close]) {
                    out.push_str(&rest[..i]);
                } else {
                    out.push_str(&rest[..=close]);
                }
                rest = &rest[close + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out
}

/// 从歌词索引文本里取出第一个含关键词的歌词行（大小写不敏感，剥掉标记后匹配，
/// 与 match_field 的歌词匹配同口径）。索引文本是整首歌词原文（扫描时写入），
/// 时间戳只是时间轴；空行（间奏）与元信息行跳过。找不到返回 None（如命中的
/// 是 [ti:] 元信息行）。
fn extract_lyric_line(blob: &str, q_lower: &str) -> Option<String> {
    for raw in blob.lines() {
        let text = strip_lyric_tags(raw).trim().to_string();
        if text.is_empty() {
            continue;
        }
        if text.to_lowercase().contains(q_lower) {
            if text.chars().count() > LYRIC_SNIPPET_MAX {
                let kept: String = text.chars().take(LYRIC_SNIPPET_MAX).collect();
                return Some(format!("{kept}…"));
            }
            return Some(text);
        }
    }
    None
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

    // 合并记忆（旧名别名 → 主艺人）：搜旧名等同于搜到合并后的主艺人。
    // 悬空别名（主艺人已删）的 id 不会出现在任何曲目的艺人列表里，天然不会误中。
    let alias_by_artist: HashMap<i64, Vec<String>> = {
        let mut stmt = conn
            .prepare("SELECT artist_id, alias FROM artist_aliases")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        let mut m: HashMap<i64, Vec<String>> = HashMap::new();
        for row in rows {
            let (aid, alias) = row.map_err(|e| e.to_string())?;
            m.entry(aid).or_default().push(alias);
        }
        m
    };

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
                    // 主艺人 + track_artists 合作艺人都命中；合并记忆的旧名别名同样算命中
                    ors.push("(IFNULL(a.name,'') LIKE ? ESCAPE '\\' \
                              OR EXISTS (SELECT 1 FROM track_artists ta JOIN artists a2 ON a2.id = ta.artist_id WHERE ta.track_id = t.id AND a2.name LIKE ? ESCAPE '\\') \
                              OR EXISTS (SELECT 1 FROM track_artists ta2 JOIN artist_aliases aa ON aa.artist_id = ta2.artist_id WHERE ta2.track_id = t.id AND aa.alias LIKE ? ESCAPE '\\') \
                              OR EXISTS (SELECT 1 FROM artist_aliases aa2 WHERE aa2.artist_id = t.artist_id AND aa2.alias LIKE ? ESCAPE '\\'))".into());
                    args.push(Box::new(like.clone()));
                    args.push(Box::new(like.clone()));
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
            let mut best = 0;
            let mut artist_matched_ids: Vec<i64> = Vec::new();
            if f.as_str() == F_ARTIST {
                // (文本, 艺人 id)：本名 + 合并别名，命中的文本记到对应艺人名下——
                // 前端只给真正命中的艺人上主题色，合作艺人不受牵连
                let mut texts: Vec<(&str, i64)> = Vec::new();
                for a in &h.track.artists {
                    texts.push((a.name.as_str(), a.id));
                    if let Some(list) = alias_by_artist.get(&a.id) {
                        for al in list {
                            texts.push((al.as_str(), a.id));
                        }
                    }
                }
                if texts.is_empty() {
                    if let Some(n) = h.track.artist.as_deref() {
                        texts.push((n, h.track.artist_id.unwrap_or(-1)));
                    }
                }
                for (text, id) in &texts {
                    let s = match_field(
                        f,
                        text,
                        &q_lower,
                        &q_full,
                        &q_init,
                        pinyin_active,
                        &mut cache,
                    );
                    if s > 0 {
                        if s > best {
                            best = s;
                        }
                        if *id >= 0 && !artist_matched_ids.contains(id) {
                            artist_matched_ids.push(*id);
                        }
                    }
                }
            } else {
                let texts: Vec<&str> = match f.as_str() {
                    F_TITLE => vec![h.track.title.as_str()],
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
            }
            if best > 0 {
                score += best;
                matched.push(f.clone());
                if f.as_str() == F_ARTIST {
                    h.track.matched_artist_ids = artist_matched_ids;
                }
            }
        }
        h.score = score;
        // 歌词字段命中：从歌词索引原文里取出命中的那一行（剥掉时间戳）随结果
        // 返回，前端据此在搜索下拉 / 结果页表格行内展示「为什么命中」
        if matched.iter().any(|f| f == F_LYRICS) {
            h.track.matched_lyric = extract_lyric_line(&h.lyrics, &q_lower);
        }
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
    use crate::commands::TrackQuery;

    #[test]
    fn escapes_like_wildcards() {
        assert_eq!(like_escape("50%_off"), "50\\%\\_off");
        assert_eq!(like_escape("a\\b"), "a\\\\b");
    }

    /// 歌词行提取：剥掉 LRC 时间戳 / A2 字级标签，跳过元信息行与空行。
    #[test]
    fn extracts_matched_lyric_line() {
        // 普通 LRC：取含关键词的行，时间戳剥掉；间奏空行跳过
        let blob = "[00:00.00]\n[00:12.34]天空之城 我们一起走过\n[00:20.00]下一句";
        assert_eq!(
            extract_lyric_line(blob, "一起").as_deref(),
            Some("天空之城 我们一起走过")
        );
        // 未命中返回 None
        assert_eq!(extract_lyric_line(blob, "不存在"), None);
        // 元信息行（[ti:]/[ar:]/[offset:]）不算歌词命中
        let meta = "[ti:一起红的歌]\n[ar:一起唱]\n[offset:+250]";
        assert_eq!(extract_lyric_line(meta, "一起"), None);
        // 增强 LRC（A2）：行内字级时间标签剥掉
        let a2 = "[00:12.00]<00:12.00>你<00:12.35>好<00:13.10>世界";
        assert_eq!(extract_lyric_line(a2, "你好").as_deref(), Some("你好世界"));
        // 多标签逐字 LRC：一行多个时间戳
        let multi = "[00:00.000]身[00:00.582]骑[00:01.164]白[00:02.328]马";
        assert_eq!(extract_lyric_line(multi, "白马").as_deref(), Some("身骑白马"));
        // 重复行写法：取第一个匹配行
        let dup = "[00:12][01:20]同一句歌词\n[02:00]另一句";
        assert_eq!(extract_lyric_line(dup, "同一句").as_deref(), Some("同一句歌词"));
        // 大小写不敏感（与 match_field 的歌词匹配同口径）
        assert_eq!(extract_lyric_line("[00:01.00]Hello Sky", "hello").as_deref(), Some("Hello Sky"));
        // 超长行截断加省略号
        let long: String = std::iter::repeat('歌').take(100).collect();
        let line = extract_lyric_line(&format!("[00:01.00]{long}"), "歌").unwrap();
        assert_eq!(line.chars().count(), LYRIC_SNIPPET_MAX + 1);
        assert!(line.ends_with('…'));
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

    /// 合并后的状态：旧艺人已删、曲目归「新艺人」、旧名记为别名。
    /// 搜旧名应命中主艺人和合作艺人的曲目（SQL 预筛 + 评分两条路径都走别名）。
    #[test]
    fn search_hits_merged_artist_by_alias() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        crate::db::register_collations(&conn).unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO sources (id, name) VALUES (1, 's');
             INSERT INTO artists (id, name) VALUES (2, '新艺人'), (3, '其他人');
             INSERT INTO tracks (id, source_id, path, title, artist_id, added_at) VALUES
               (1, 1, 'p1', 'T1', 2, 100),
               (2, 1, 'p2', 'T2', 3, 100);
             INSERT INTO track_artists (track_id, artist_id, ord) VALUES
               (1, 2, 0),
               (2, 3, 0), (2, 2, 1);
             INSERT INTO artist_aliases (alias, artist_id) VALUES ('旧艺人', 2);",
        )
        .unwrap();

        let q = TrackQuery {
            search: Some("旧艺人".into()),
            fields: Some(vec!["artist".into()]),
            pinyin: Some(false),
            ..no_view()
        };
        let page = search_tracks(&conn, &q).unwrap();
        assert_eq!(page.total, 2);
        let ids: Vec<i64> = page.items.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![1, 2]);
        assert!(page.items[0].matched_fields.contains(&"artist".to_string()));
        // 精确到艺人：命中的是「新艺人」（含合作曲目），「其他人」不被牵连
        assert_eq!(page.items[0].matched_artist_ids, vec![2]);
        assert_eq!(page.items[1].matched_artist_ids, vec![2]);

        // 不相关关键词不命中
        let q = TrackQuery {
            search: Some("不存在的艺人".into()),
            fields: Some(vec!["artist".into()]),
            pinyin: Some(false),
            ..no_view()
        };
        let page = search_tracks(&conn, &q).unwrap();
        assert_eq!(page.total, 0);
    }

    /// 歌词搜索：命中字段标记 + 命中的歌词行（剥时间戳）随结果返回。
    #[test]
    fn search_returns_matched_lyric_line() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        crate::db::register_collations(&conn).unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO sources (id, name) VALUES (1, 's');
             INSERT INTO tracks (id, source_id, path, title, added_at) VALUES (1, 1, 'p1', 'T1', 100);
             INSERT INTO lyrics_index (track_id, text) VALUES (1, '[00:00.00]前奏\n[00:12.34]天空之城 一起走过的日子\n[00:30.00]尾声');",
        )
        .unwrap();

        let q = TrackQuery {
            search: Some("走过".into()),
            fields: Some(vec!["lyrics".into()]),
            pinyin: Some(false),
            ..no_view()
        };
        let page = search_tracks(&conn, &q).unwrap();
        assert_eq!(page.total, 1);
        assert!(page.items[0].matched_fields.contains(&"lyrics".to_string()));
        assert_eq!(
            page.items[0].matched_lyric.as_deref(),
            Some("天空之城 一起走过的日子")
        );

        // 未命中歌词的搜索不带歌词行
        let q = TrackQuery {
            search: Some("T1".into()),
            fields: Some(vec!["title".into()]),
            pinyin: Some(false),
            ..no_view()
        };
        let page = search_tracks(&conn, &q).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].matched_lyric, None);
    }

    fn no_view() -> TrackQuery {
        TrackQuery {
            view: None,
            ref_id: None,
            search: None,
            sort: None,
            page: None,
            page_size: None,
            fields: None,
            pinyin: None,
        }
    }
}
