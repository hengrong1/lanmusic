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

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};

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

            // 系统托盘（M2）
            let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let toggle = MenuItem::with_id(app, "toggle", "播放 / 暂停", true, None::<&str>)?;
            let prev = MenuItem::with_id(app, "prev", "上一首", true, None::<&str>)?;
            let next = MenuItem::with_id(app, "next", "下一首", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 LanMusic", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &toggle, &prev, &next, &quit])?;
            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().expect("缺少应用图标").clone())
                .tooltip("LanMusic")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                        }
                    }
                    "toggle" => {
                        let _ = app.emit("tray", "toggle");
                    }
                    "prev" => {
                        let _ = app.emit("tray", "prev");
                    }
                    "next" => {
                        let _ = app.emit("tray", "next");
                    }
                    "quit" => app.exit(0),
                    _ => {}
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
            commands::webdav_add_source
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
