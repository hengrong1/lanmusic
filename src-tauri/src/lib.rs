mod commands;
mod covers;
#[cfg(windows)]
mod fonts;
mod db;
mod keyring;
mod lyrics;
mod metadata;
mod network;
mod scanner;
mod scheme;
mod state;
#[cfg(windows)]
mod thumbbar;
mod watcher;

use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例：二次启动时唤起已运行实例的主窗口（官方建议此插件最先注册）
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        // 应用内更新：检查/下载 GitHub Releases 的更新包（签名校验见 tauri.conf.json 的 pubkey）
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // 主窗口：在 Rust 侧按平台创建（官方《窗口自定义》推荐做法）
            // - macOS：保留原生红绿灯，仅透明化标题栏（fullSizeContentView），内容延伸到标题栏下方
            // - Windows/Linux：完全无边框（decorations: false），由前端自绘控制按钮
            let win_builder = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                .title("LanMusic")
                .inner_size(1280.0, 820.0)
                .min_inner_size(980.0, 640.0)
                .center();
            #[cfg(target_os = "macos")]
            let win_builder = win_builder.title_bar_style(tauri::TitleBarStyle::Transparent);
            #[cfg(not(target_os = "macos"))]
            let win_builder = win_builder.decorations(false);
            #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
            let window = win_builder.build().map_err(|e| e.to_string())?;

            // macOS：设置原生窗口背景色（跟随系统深浅色外观），
            // 避免启动首帧白闪，以及窗口圆角处露出默认底色
            #[cfg(target_os = "macos")]
            {
                use objc2_app_kit::{NSColor, NSWindow};
                let ns_window_ptr = window.ns_window().unwrap() as *mut NSWindow;
                let ns_window = unsafe { &*ns_window_ptr };
                ns_window.setBackgroundColor(Some(&NSColor::windowBackgroundColor()));
            }

            // 关闭窗口行为：根据用户设置决定是最小化到托盘还是退出应用
            // 前端通过 set_setting 存储 lm.closeAction（'tray' 或 'quit'）到 SQLite
            {
                let app_handle = app.handle().clone();
                let main_window = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // 读取用户设置：默认最小化到托盘。
                        // 读取失败（数据库锁被污染/被占用）时同样按「最小化到托盘」处理，
                        // 不能退回默认行为——默认行为只销毁主窗口，进程不会退出。
                        let should_quit = match app_handle.state::<crate::state::AppState>().db.lock() {
                            Ok(guard) => {
                                crate::db::get_setting(&guard, "lm.closeAction").as_deref()
                                    == Some("quit")
                            }
                            Err(_) => false,
                        };

                        if should_quit {
                            // 用户选择关闭时退出应用。
                            // 注意：不能直接依赖默认行为（销毁主窗口）——托盘菜单窗口 tray
                            // 是常驻的隐藏窗口，桌面歌词窗口也可能开着，只要还有窗口存活
                            // Tauri 就不会退出进程，托盘图标会残留在系统托盘区。
                            // 因此这里必须显式 exit；放到子线程执行，避免在窗口事件回调
                            // 中重入事件循环造成死锁。
                            api.prevent_close();
                            let handle = app_handle.clone();
                            std::thread::spawn(move || {
                                handle.exit(0);
                            });
                            return;
                        }

                        // 默认行为：隐藏窗口到托盘，不退出
                        api.prevent_close();
                        let _ = main_window.hide();
                    }
                });
            }

            let data_dir = app.path().app_data_dir().expect("无法获取应用数据目录");
            std::fs::create_dir_all(&data_dir)?;
            let covers_dir = data_dir.join("covers");
            std::fs::create_dir_all(&covers_dir)?;
            let db_path = data_dir.join("library.db");
            let conn = db::open(&db_path).expect("无法打开数据库");

            // WebDAV 凭证一次性迁移：旧版本明文存于 sources.config，迁入系统钥匙串
            keyring::migrate_plaintext(&conn);

            // 一次性自愈：旧版本删除专辑时未清理 covers/{id}.jpg，而 SQLite 会复用已删除
            // 专辑的 rowid，导致新专辑命中旧封面（表现为部分歌曲显示别人的封面）。
            // 存量缓存已被污染且无法与专辑一一对应，启动时清空一次，之后惰性重建。
            let covers_selfheal: Option<String> = conn
                .query_row(
                    "SELECT value FROM app_settings WHERE key = 'covers.selfheal.v1'",
                    [],
                    |r| r.get(0),
                )
                .ok();
            if covers_selfheal.is_none() {
                let _ = std::fs::remove_dir_all(&covers_dir);
                std::fs::create_dir_all(&covers_dir)?;
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('covers.selfheal.v1', '1')",
                    [],
                );
            }

            app.manage(state::AppState {
                db: std::sync::Mutex::new(conn),
                db_path,
                covers_dir,
                scanning: std::sync::Mutex::new(std::collections::HashSet::new()),
                cover_extract: std::sync::Mutex::new(()),
                watcher: std::sync::Mutex::new(watcher::WatchState::default()),
            });

            // 本地来源目录监听：文件变化后自动增量扫描（WebDAV 源无法监听）
            {
                let state = app.state::<state::AppState>();
                let conn = state.db.lock().map_err(|e| e.to_string())?;
                let rows: Vec<(i64, String)> = {
                    let mut stmt = conn
                        .prepare("SELECT id, base_path FROM sources WHERE kind = 'local' AND base_path IS NOT NULL")
                        .map_err(|e| e.to_string())?;
                    let rows = stmt
                        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                        .map_err(|e| e.to_string())?;
                    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
                };
                drop(conn);
                for (id, base) in rows {
                    watcher::watch_source(app.handle(), id, &base);
                }
            }
            watcher::init(app.handle().clone());

            // 封面缓存容量控制：启动时清理一次（后台执行，扫描结束时也会再执行）
            {
                let app_handle = app.handle().clone();
                std::thread::spawn(move || covers::enforce_limit_with_setting(&app_handle));
            }

            // 系统托盘：点击图标弹出自定义菜单弹窗（类似 QQ 音乐）。
            // 原生托盘菜单无法实现圆角/封面/悬停高亮等样式，改用独立 tray 窗口渲染；
            // 启动时预创建为隐藏窗口，点击即现，失焦自动收起。
            let tray_builder =
                tauri::WebviewWindowBuilder::new(app, "tray", tauri::WebviewUrl::App("index.html".into()))
                    .title("LanMusic 托盘菜单")
                    .decorations(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .resizable(false)
                    .shadow(false)
                    .focused(false)
                    .visible(false)
                    .inner_size(TRAY_MENU_W, TRAY_MENU_H);
            // 透明背景：Windows/Linux 支持圆角+投影；macOS 需 macos-private-api feature，v1 暂不启用
            #[cfg(any(windows, target_os = "linux"))]
            let tray_builder = tray_builder.transparent(true);
            let tray_window = tray_builder.build()?;

            // 失焦自动收起：延迟 150ms，避免「点击托盘关闭菜单」时 blur 先隐藏、
            // 随后托盘 Click 事件又把菜单重新弹出的竞态
            {
                let w = tray_window.clone();
                tray_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        let w = w.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(150));
                            if !w.is_focused().unwrap_or(true) {
                                let _ = w.hide();
                            }
                        });
                    }
                });
            }

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().expect("缺少应用图标").clone())
                .tooltip("LanMusic")
                .on_tray_icon_event(|tray, event| {
                    // 左键：根据主窗口状态决定还原/前置（最小化 → 还原；可见但被遮 → 前置聚焦）
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        focus_or_show_main_window(tray.app_handle());
                        return;
                    }
                    // 右键（或中键）：切换自定义菜单弹窗（原行为保留）
                    if let TrayIconEvent::Click {
                        position,
                        rect,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_tray_menu(tray.app_handle(), position, rect);
                    }
                })
                .build(app)?;

            // Windows：任务栏缩略图工具栏（悬停任务栏图标时的 上一首/播放暂停/下一首）
            #[cfg(windows)]
            if let Ok(hwnd) = window.hwnd() {
                thumbbar::init(app.handle().clone(), hwnd);
            }

            Ok(())
        })
        // 音频流协议：music://track/{id}（Windows 上为 http://music.localhost/track/{id}）
        .register_asynchronous_uri_scheme_protocol("music", scheme::music_protocol)
        // 封面协议：cover://album/{album_id}
        .register_asynchronous_uri_scheme_protocol("cover", scheme::cover_protocol)
        // 视频流协议：video://mv/{track_id}（Windows 上为 http://video.localhost/mv/{track_id}）
        .register_asynchronous_uri_scheme_protocol("video", scheme::video_protocol)
        .invoke_handler(tauri::generate_handler![
            commands::add_local_source,
            commands::list_sources,
            commands::remove_source,
            commands::rescan_source,
            commands::set_source_fast_import,
            commands::query_tracks,
            commands::query_albums,
            commands::query_artists,
            commands::get_track,
            commands::get_tracks_by_ids,
            commands::get_stream_url,
            commands::library_stats,
            commands::query_genres,
            commands::reveal_track,
            commands::playlist_list,
            commands::playlist_create,
            commands::playlist_rename,
            commands::playlist_set_description,
            commands::playlist_delete,
            commands::playlist_get_items,
            commands::playlist_add_tracks,
            commands::playlist_remove_track,
            commands::playlist_remove_tracks,
            commands::playlist_reorder,
            commands::playlist_cover,
            commands::report_play,
            commands::get_lyrics,
            commands::favorite_toggle,
            commands::get_setting,
            commands::set_setting,
            commands::set_thumbbar_playing,
            commands::desktop_lyrics_set,
            commands::list_system_fonts,
            commands::exit_app,
            commands::set_prevent_sleep,
            commands::webdav_add_source,
            commands::get_mv_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 托盘菜单弹窗逻辑尺寸（物理像素 = 逻辑 × DPI 缩放）
const TRAY_MENU_W: f64 = 288.0;
/// 初始高度（内容自然高度约 183）。前端挂载后会按实际内容高度调用 setSize 校正，
/// 这里只是首帧兜底值；定位统一用窗口当前真实尺寸，不依赖本常量。
const TRAY_MENU_H: f64 = 184.0;

/// 托盘点击：菜单弹窗已显示则收起，否则按托盘图标位置弹出
fn toggle_tray_menu(app: &AppHandle, cursor: PhysicalPosition<f64>, tray_rect: tauri::Rect) {
    let Some(w) = app.get_webview_window("tray") else {
        return;
    };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    position_tray_menu(&w, cursor, &tray_rect);
    let _ = w.show();
    let _ = w.set_focus();
}

/// 单击托盘图标（左键）：按主窗口当前状态决定是还原还是前置聚焦
///
/// - 隐藏或最小化：`show() + unminimize()` 还原
/// - 可见但被其它窗口遮挡：`set_focus()` 前置到 Z 序顶部（`SetForegroundWindow`）
/// - 已经在最前：不操作，避免误隐藏
///
/// 顺带关掉可能正显示的菜单弹窗，避免「主窗口浮起来 + 菜单弹窗还盖在上面」的怪态。
fn focus_or_show_main_window(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    if let Some(tray) = app.get_webview_window("tray") {
        if tray.is_visible().unwrap_or(false) {
            let _ = tray.hide();
        }
    }
    let minimized = w.is_minimized().unwrap_or(false);
    let visible = w.is_visible().unwrap_or(false);
    if minimized || !visible {
        let _ = w.show();
        let _ = w.unminimize();
    }
    let _ = w.set_focus();
}

/// 把弹窗定位到托盘图标上方居中（任务栏在顶部时改为下方），并 clamp 在光标所在显示器内
fn position_tray_menu(
    w: &tauri::WebviewWindow,
    cursor: PhysicalPosition<f64>,
    tray_rect: &tauri::Rect,
) {
    // 光标所在显示器（多显示器时弹窗与托盘同屏）
    let monitor = w
        .monitor_from_point(cursor.x, cursor.y)
        .ok()
        .flatten()
        .or_else(|| w.primary_monitor().ok().flatten());
    let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
    // 用窗口当前真实尺寸定位：前端会按内容高度收缩窗口（去掉底部空白），
    // 若仍按常量计算，菜单底边会偏离托盘图标（空隙变大）。
    let (pw, ph) = match w.inner_size() {
        Ok(s) => (s.width as i32, s.height as i32),
        Err(_) => (
            (TRAY_MENU_W * scale) as i32,
            (TRAY_MENU_H * scale) as i32,
        ),
    };
    // 托盘图标矩形：事件的 position/size 为逻辑/物理混合的枚举，统一转物理像素
    let icon_pos = tray_rect.position.to_physical::<i32>(scale);
    let icon = tray_rect.size.to_physical::<u32>(scale);

    // 水平：托盘图标中心对齐弹窗中心；垂直：图标上方留 8 物理像素
    let icon_cx = icon_pos.x + icon.width as i32 / 2;
    let mut x = icon_cx - pw / 2;
    let mut y = icon_pos.y - ph - 8;
    if let Some(m) = &monitor {
        let mp = m.position();
        let ms = m.size();
        x = x.clamp(mp.x + 8, mp.x + ms.width as i32 - pw - 8);
        if y < mp.y + 8 {
            // 任务栏在屏幕顶部：弹窗改到图标下方
            y = icon_pos.y + icon.height as i32 + 8;
        }
    }
    let _ = w.set_position(PhysicalPosition::new(x, y));
}
