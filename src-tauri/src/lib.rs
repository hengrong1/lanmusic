mod commands;
mod covers;
#[cfg(windows)]
mod fonts;
mod db;
mod lyrics;
mod metadata;
mod network;
mod scanner;
mod scheme;
mod state;
#[cfg(windows)]
mod thumbbar;

use tauri::tray::{MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
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

            let data_dir = app.path().app_data_dir().expect("无法获取应用数据目录");
            std::fs::create_dir_all(&data_dir)?;
            let covers_dir = data_dir.join("covers");
            std::fs::create_dir_all(&covers_dir)?;
            let db_path = data_dir.join("library.db");
            let conn = db::open(&db_path).expect("无法打开数据库");

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
            });

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
            commands::webdav_add_source
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 托盘菜单弹窗逻辑尺寸（物理像素 = 逻辑 × DPI 缩放）
const TRAY_MENU_W: f64 = 288.0;
const TRAY_MENU_H: f64 = 196.0;

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
    let pw = (TRAY_MENU_W * scale) as i32;
    let ph = (TRAY_MENU_H * scale) as i32;
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
