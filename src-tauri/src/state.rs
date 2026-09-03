use rusqlite::Connection;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::watcher;

pub struct AppState {
    /// UI 读写连接：命令查询快进快出（毫秒级持锁）
    pub db: Mutex<Connection>,
    /// 数据库文件路径：扫描线程各自开独立写连接，WAL 下与 UI 读互不阻塞
    pub db_path: PathBuf,
    /// 专辑封面缓存目录（covers/{album_id}.jpg / {album_id}.none 哨兵）
    pub covers_dir: PathBuf,
    /// 正在扫描的 source_id，防止并发重复扫描
    pub scanning: Mutex<HashSet<i64>>,
    /// 封面惰性提取串行化：专辑网格首屏会触发大量封面请求，避免并发网络读风暴
    pub cover_extract: Mutex<()>,
    /// 本地来源目录监听状态（watcher.rs）
    pub watcher: Mutex<watcher::WatchState>,
}
