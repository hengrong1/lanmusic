use rusqlite::Connection;
use std::path::Path;

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

CREATE TABLE IF NOT EXISTS app_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
"#;

/// 打开连接并初始化（建表 + 迁移）。扫描线程用 init=false，只设 PRAGMA。
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    open_conn(path, true)
}

pub fn open_conn(path: &Path, init: bool) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    if init {
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
    }
    Ok(conn)
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    // meta_state: 0=快速导入（仅文件名入库），1=完整解析过标签
    ensure_column(conn, "tracks", "meta_state", "INTEGER NOT NULL DEFAULT 1")?;
    ensure_column(conn, "sources", "fast_import", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(conn, "tracks", "remote_id", "INTEGER")?;
    ensure_column(conn, "albums", "remote_id", "INTEGER")?;
    ensure_column(conn, "albums", "cover_url", "TEXT")?;
    ensure_column(conn, "sources", "config", "TEXT")?;
    ensure_column(conn, "tracks", "fav", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(conn, "playlist_items", "added_at", "INTEGER")?;
    ensure_column(conn, "playlists", "description", "TEXT")?;
    // 旧数据无加入时间：回填 0 视为最早加入，倒序时排在最前
    conn.execute("UPDATE playlist_items SET added_at = 0 WHERE added_at IS NULL", [])?;

    // LAN 共享功能已移除：清理遗留的 lan 来源（曲目经外键级联删除），
    // 并回收因此产生的孤儿专辑/艺人（含仅被专辑引用的归属艺人）
    let lan_removed = conn.execute("DELETE FROM sources WHERE kind = 'lan'", [])?;
    if lan_removed > 0 {
        conn.execute("DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)", [])?;
        conn.execute(
            "DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM tracks)
             AND id NOT IN (SELECT DISTINCT artist_id FROM albums)",
            [],
        )?;
    }
    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, column: &str, def: &str) -> rusqlite::Result<()> {
    let sql = format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'");
    let exists: i64 = conn.query_row(&sql, [], |r| r.get(0))?;
    if exists == 0 {
        conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {def}"), [])?;
    }
    Ok(())
}

// ---------- 应用设置（KV） ----------

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM app_settings WHERE key = ?1", [key], |r| r.get(0))
        .ok()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) {
    let _ = conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    );
}
