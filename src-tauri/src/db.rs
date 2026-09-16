use pinyin::ToPinyin;
use rusqlite::Connection;
use std::path::Path;

/// 拼音排序 key（分组式）：数字 < 字母 < 汉字，组内各按次序。
/// 汉字逐字符转无声调全拼（多音字取默认音），并在每个汉字前插入哨兵字符
/// （U+10FFFD，仅次于 Unicode 非字符的极大码点）——汉字域整体大于数字/字母/常见符号，
/// 因此「Adele」排在「阿杜」之前（组分组），而「阿杜 vs 周杰伦」仍按拼音比（哨兵相同比拼音）。
/// 非汉字字符 Unicode 小写原样保留，涵盖原 NOCASE 大小写不敏感语义。
/// 连续 ASCII 数字段转定宽（12 位右对齐补零）token：段内按数值排序（"2" < "10"），
/// 超 12 位的段取末 12 位（降级为模 10^12 序，曲名数字段不会这么长）；
/// 注意 "1" 与 "01" 的 key 相等（数值等价），相等行顺序由 SQLite 决定。
/// 空值兜底（SQL 侧 IFNULL(x, CHAR(1114110)) = U+10FFFE）大于汉字哨兵，无专辑/艺人绝对排最后。
/// 注意：混合串（如「AB张三」）按逐字符映射自然分段，前缀段（字母）先比，行为符合直觉。
fn pinyin_key(s: &str) -> String {
    // 汉字域哨兵：大于一切常规文本字符（仅 U+10FFFE/FFFF 非字符在其后，实际文本不出现；
    // SQL 空值兜底即借用 U+10FFFE，见 commands.rs 排序分支）
    const HAN_SENTINEL: char = '\u{10FFFD}';
    /// 数字段定宽位数
    const NUM_WIDTH: usize = 12;
    let mut out = String::with_capacity(s.len() * 2);
    let mut digits = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if !digits.is_empty() {
            flush_digit_segment(&mut out, &mut digits, NUM_WIDTH);
        }
        if let Some(py) = c.to_pinyin() {
            out.push(HAN_SENTINEL);
            out.push_str(py.plain());
        } else {
            out.extend(c.to_lowercase());
        }
    }
    if !digits.is_empty() {
        flush_digit_segment(&mut out, &mut digits, NUM_WIDTH);
    }
    out
}

/// 把攒下的数字段以定宽补零形式写入 key（右对齐，前导补 '0'），
/// 使段间数值序 == 字节序；补零后首字符仍是 '0'，保持数字组排在字母组之前的分组语义。
fn flush_digit_segment(out: &mut String, digits: &mut String, width: usize) {
    // 超宽段取末 width 位（ASCII 数字按字节切片安全）
    let start = digits.len().saturating_sub(width);
    let seg = &digits[start..];
    for _ in 0..width - seg.len() {
        out.push('0');
    }
    out.push_str(seg);
    digits.clear();
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sources (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL DEFAULT 'local',
  name TEXT NOT NULL,
  base_path TEXT,
  base_url TEXT,
  config TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  last_scan_at INTEGER
);

CREATE TABLE IF NOT EXISTS artists (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

-- 艺人别名（合并记忆）：旧名/别名 → 主艺人。规整与自定义合并后写入，
-- 扫描再遇到旧名时仍归到主艺人名下。刻意不加外键：主艺人被删除后别名仍在
-- （按名字解析），同名主艺人重建时别名继续生效。
CREATE TABLE IF NOT EXISTS artist_aliases (
  alias TEXT NOT NULL UNIQUE COLLATE NOCASE,
  artist_id INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_artist_aliases_artist ON artist_aliases(artist_id);

CREATE TABLE IF NOT EXISTS albums (
  id INTEGER PRIMARY KEY,
  title TEXT NOT NULL,
  artist_id INTEGER REFERENCES artists(id),
  year INTEGER,
  has_cover INTEGER NOT NULL DEFAULT 0,
  key TEXT NOT NULL UNIQUE,
  remote_id INTEGER,
  cover_url TEXT
);

CREATE TABLE IF NOT EXISTS tracks (
  id INTEGER PRIMARY KEY,
  source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
  path TEXT NOT NULL,
  title TEXT NOT NULL,
  artist_id INTEGER REFERENCES artists(id),
  album_id INTEGER REFERENCES albums(id),
  genre TEXT,
  track_no INTEGER,
  disc_no INTEGER,
  year INTEGER,
  duration REAL,
  bitrate INTEGER,
  sample_rate INTEGER,
  channels INTEGER,
  bit_depth INTEGER,
  has_embedded_lyrics INTEGER NOT NULL DEFAULT 0,
  fav INTEGER NOT NULL DEFAULT 0,
  mtime INTEGER,
  file_size INTEGER,
  format TEXT,
  added_at INTEGER,
  play_count INTEGER NOT NULL DEFAULT 0,
  last_played_at INTEGER,
  remote_id INTEGER,
  UNIQUE (source_id, path)
);
CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album_id);
CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist_id);

-- 曲目 ↔ 艺人多对多关联（"A / B" 这类合作曲目拆成独立艺人，ord 记录展示顺序）
CREATE TABLE IF NOT EXISTS track_artists (
  track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
  artist_id INTEGER NOT NULL REFERENCES artists(id),
  ord INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (track_id, artist_id)
);
CREATE INDEX IF NOT EXISTS idx_track_artists_artist ON track_artists(artist_id);

CREATE TABLE IF NOT EXISTS playlists (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  created_at INTEGER,
  sort INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS playlist_items (
  id INTEGER PRIMARY KEY,
  playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
  track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
  position INTEGER NOT NULL DEFAULT 0
);

-- 外挂歌词：path 为本地文件路径（local）或完整 URL（webdav）；NULL 表示走远程接口（lan）
CREATE TABLE IF NOT EXISTS lrc_files (
  track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
  path TEXT
);

-- 歌词搜索索引：扫描时写入（内嵌歌词 + 外挂 .lrc 内容），存量库启动时后台回填
CREATE TABLE IF NOT EXISTS lyrics_index (
  track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
  text TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS app_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

-- 已移除歌曲记录：手动从曲库移除 / 扫描发现文件消失时写入（仅记录曲目信息便于事后查找，
-- 不含文件本身）。超出上限按时间从旧到新裁剪（见 cap_removed_log）。
CREATE TABLE IF NOT EXISTS removed_tracks (
  id INTEGER PRIMARY KEY,
  title TEXT NOT NULL,
  artist TEXT,
  album TEXT,
  path TEXT NOT NULL,
  source_id INTEGER,
  reason TEXT NOT NULL DEFAULT 'manual',
  removed_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_removed_tracks_at ON removed_tracks(removed_at DESC, id DESC);
"#;

/// 移除记录上限：超出时按时间从旧到新裁剪
pub const REMOVED_LOG_CAP: i64 = 1000;

/// 裁剪移除记录到上限之内
pub fn cap_removed_log(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM removed_tracks WHERE id NOT IN \
         (SELECT id FROM removed_tracks ORDER BY removed_at DESC, id DESC LIMIT ?1)",
        [REMOVED_LOG_CAP],
    )
    .map(|_| ())
}

/// 打开连接并初始化（建表 + 迁移）。扫描线程用 init=false，只设 PRAGMA。
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    open_conn(path, true)
}

pub fn open_conn(path: &Path, init: bool) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // 中文按拼音排序：SQLite 内建 NOCASE 只对 ASCII 生效，中文按码点（部首笔画序）排
    // 不符合直觉，注册自定义 collation 供「按标题/专辑/艺人」等 ORDER BY 使用。
    // 比较时即时转换（无缓存）：万首规模单次排序在几十 ms 内，够用；
    // 库规模显著增长后再考虑入库时预计算拼音辅助列。
    conn.create_collation("PINYIN", |a: &str, b: &str| pinyin_key(a).cmp(&pinyin_key(b)))?;
    if init {
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
    }
    Ok(conn)
}

/// 启动迁移：补列 + 一次性数据修复（幂等，重复执行无副作用）。
/// 仅 UI 主连接（`open`/`open_conn(init=true)`，即启动时）执行；
/// 扫描线程开独立连接走 `open_conn(init=false)`，不经过这里。
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    // has_mv: 是否存在同名视频文件（MV）
    ensure_column(conn, "tracks", "has_mv", "INTEGER NOT NULL DEFAULT 0")?;
    // meta_state: 0=快速导入（仅文件名入库），1=完整解析过标签
    ensure_column(conn, "tracks", "meta_state", "INTEGER NOT NULL DEFAULT 1")?;
    ensure_column(conn, "sources", "fast_import", "INTEGER NOT NULL DEFAULT 0")?;
    // scan_subdirs: 是否扫描来源内子目录（0 = 仅扫描根目录下的文件）
    ensure_column(
        conn,
        "sources",
        "scan_subdirs",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    ensure_column(conn, "tracks", "remote_id", "INTEGER")?;
    ensure_column(conn, "albums", "remote_id", "INTEGER")?;
    ensure_column(conn, "albums", "cover_url", "TEXT")?;
    ensure_column(conn, "sources", "config", "TEXT")?;
    ensure_column(conn, "tracks", "fav", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(conn, "playlist_items", "added_at", "INTEGER")?;
    ensure_column(conn, "playlists", "description", "TEXT")?;
    // 原始艺人标签（未按分隔符拆分），用于调整分隔符后重新拆分艺人
    ensure_column(conn, "tracks", "raw_artist", "TEXT")?;
    // 旧数据无加入时间：回填 0 视为最早加入，倒序时排在最前
    conn.execute(
        "UPDATE playlist_items SET added_at = 0 WHERE added_at IS NULL",
        [],
    )?;

    // LAN 共享功能已移除：清理遗留的 lan 来源（曲目经外键级联删除），
    // 并回收因此产生的孤儿专辑/艺人（含仅被专辑引用的归属艺人）
    let lan_removed = conn.execute("DELETE FROM sources WHERE kind = 'lan'", [])?;
    if lan_removed > 0 {
        log::info!("迁移：清理遗留 LAN 来源 {lan_removed} 个（功能已移除）");
        conn.execute(
            "DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)",
            [],
        )?;
        conn.execute(
            "DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM tracks)
             AND id NOT IN (SELECT DISTINCT artist_id FROM track_artists)
             AND id NOT IN (SELECT DISTINCT artist_id FROM albums)",
            [],
        )?;
    }

    // 多艺人拆分（track_artists）上线：已完整解析过的曲目需重新读标签才能按
    // 分隔符拆出独立艺人。置回 meta_state=0 让下次扫描自动重解析（一次性）。
    if get_setting(conn, "track_artists_migrated").is_none() {
        log::info!("迁移：多艺人拆分上线，全部已解析曲目置回待补全（meta_state=0），下次扫描自动重解析");
        conn.execute("UPDATE tracks SET meta_state = 0", [])?;
        set_setting(conn, "track_artists_migrated", "1")?;
    }

    // raw_artist（原始艺人标签）上线：没有该值的已解析行需要重新读标签回填，
    // 之后调整艺人分隔符时可直接基于它重拆，无需再读文件（一次性）。
    if get_setting(conn, "raw_artist_migrated").is_none() {
        log::info!("迁移：raw_artist 上线，已解析行将在下次扫描回填原始艺人标签");
        conn.execute(
            "UPDATE tracks SET meta_state = 0 WHERE raw_artist IS NULL AND meta_state = 1",
            [],
        )?;
        set_setting(conn, "raw_artist_migrated", "1")?;
    }
    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, column: &str, def: &str) -> rusqlite::Result<()> {
    let sql = format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'");
    let exists: i64 = conn.query_row(&sql, [], |r| r.get(0))?;
    if exists == 0 {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {def}"),
            [],
        )?;
        // 真正执行的补列才记日志：迁移历史一目了然（存量库首次升级时会连续出现多条）
        log::info!("迁移：{table} 新增列 {column}");
    }
    Ok(())
}

// ---------- 应用设置（KV） ----------

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        [key],
        |r| r.get(0),
    )
    .ok()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )
    .map(|_| ())
}
