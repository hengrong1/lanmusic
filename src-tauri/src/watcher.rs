//! 本地来源目录监听（notify）：文件变化 → 去抖 → 自动触发增量扫描。
//!
//! 仅支持本地来源；WebDAV 源无法监听（远端文件系统变化对本机不可见），
//! 如需同步远端可手动「重新扫描」。
//! 去抖策略：最后一次文件事件静默 DEBOUNCE 后才扫描，复制大目录时不会反复触发。

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use tauri::{AppHandle, Manager};

use crate::scanner;
use crate::state::AppState;

/// 最后一次文件事件后静默多久才触发扫描
const DEBOUNCE: Duration = Duration::from_secs(3);
/// 去抖检查线程的轮询间隔
const POLL: Duration = Duration::from_millis(500);

/// 挂在 AppState 上的监听状态（见 state.rs）
#[derive(Default)]
pub struct WatchState {
    /// source_id → watcher：必须持有实例，drop 即停止监听
    watchers: HashMap<i64, notify::RecommendedWatcher>,
    /// 有待处理事件的来源 → 最后一次事件时间
    dirty: HashMap<i64, Instant>,
}

/// 为本地来源开启递归监听（add_local_source 与启动时调用）
pub fn watch_source<R: tauri::Runtime>(app: &AppHandle<R>, source_id: i64, base_path: &str) {
    let app2 = app.clone();
    let result = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
        // 任何类型的事件（创建/修改/删除/重命名）都视为「需要重扫」
        if res.is_ok() {
            let state = app2.state::<AppState>();
            if let Ok(mut w) = state.watcher.lock() {
                w.dirty.insert(source_id, Instant::now());
            };
        }
    });
    let mut watcher = match result {
        Ok(w) => w,
        Err(e) => {
            eprintln!("目录监听不可用（source {source_id}）：{e}");
            return;
        }
    };
    if let Err(e) = watcher.watch(Path::new(base_path), RecursiveMode::Recursive) {
        eprintln!("监听目录失败（{base_path}）：{e}");
        return;
    }
    let state = app.state::<AppState>();
    if let Ok(mut w) = state.watcher.lock() {
        w.watchers.insert(source_id, watcher);
    };
}

/// 停止监听（remove_source 时调用）
pub fn unwatch_source<R: tauri::Runtime>(app: &AppHandle<R>, source_id: i64) {
    let state = app.state::<AppState>();
    if let Ok(mut w) = state.watcher.lock() {
        w.watchers.remove(&source_id);
        w.dirty.remove(&source_id);
    };
}

/// 启动去抖线程：静默期满的来源触发增量扫描（不与手动扫描并发，正在扫描则重新进入去抖）
pub fn init(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(POLL);
        let due: Vec<i64> = {
            let state = app.state::<AppState>();
            let Ok(mut w) = state.watcher.lock() else { continue };
            let now = Instant::now();
            let due: Vec<i64> = w
                .dirty
                .iter()
                .filter(|(_, t)| now.duration_since(**t) >= DEBOUNCE)
                .map(|(id, _)| *id)
                .collect();
            for id in &due {
                w.dirty.remove(id);
            }
            due
        };
        for id in due {
            let state = app.state::<AppState>();
            let mut scanning = match state.scanning.lock() {
                Ok(s) => s,
                Err(_) => continue,
            };
            if scanning.contains(&id) {
                // 正在扫描：本次扫描结束后可能还有新事件，重新进入去抖等待
                if let Ok(mut w) = state.watcher.lock() {
                    w.dirty.insert(id, Instant::now());
                }
                continue;
            }
            scanning.insert(id);
            drop(scanning);
            let app2 = app.clone();
            std::thread::spawn(move || scanner::scan_source(app2, id, false));
        }
    });
}