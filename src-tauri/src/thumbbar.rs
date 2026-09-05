//! Windows 任务栏缩略图工具栏（ThumbBar）：
//! 鼠标悬停任务栏图标时，在缩略图下方显示「上一首 / 播放暂停 / 下一首」控制按钮（类似 QQ 音乐）。
//!
//! 实现方式：Win32 `ITaskbarList3::ThumbBarAddButtons` + 窗口过程子类化。
//! - 按钮图标：代码绘制的形状（无需图片资源），颜色跟随系统浅/深色主题——
//!   浅色主题按钮底色浅，用深色图标；深色主题用白色图标。经 `CreateDIBSection`
//!   + `CreateIconIndirect` 生成带透明通道的 HICON，尺寸按窗口 DPI 缩放（16dp 基准）。
//! - 点击按钮：系统发来 `WM_COMMAND`（LOWORD(wParam) 为按钮 id），转发为现有 `tray`
//!   事件（prev / toggle / next），前端播放器监听同一事件驱动播放。
//! - 图标同步：前端播放状态变化时调用 `set_thumbbar_playing` 命令，切换中间按钮的
//!   播放/暂停图标。
//! - `TaskbarButtonCreated`：任务栏按钮首次创建或 explorer 重启后重新添加按钮。
//!
//! 悬停预览封面：主窗口开启 DWM「图标式缩略图」（DWMWA_HAS_ICONIC_BITMAP +
//! DWMWA_FORCE_ICONIC_REPRESENTATION）后，任务栏缩略图 / Aero Peek / Alt+Tab 不再抓取
//! 窗口画面，而是由 DWM 发私有消息（0x0323 / 0x0326）索取位图；本模块据此把当前歌曲
//! 封面整块铺进预览区域。无封面时属性保持关闭，缩略图回退为系统默认的真实窗口内容。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use image::RgbaImage;
use tauri::{AppHandle, Emitter};
use windows::core::w;
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmInvalidateIconicBitmaps, DwmSetIconicLivePreviewBitmap, DwmSetIconicThumbnail,
    DwmSetWindowAttribute, DWMWA_FORCE_ICONIC_REPRESENTATION, DWMWA_HAS_ICONIC_BITMAP,
    DWM_SIT_DISPLAYFRAME,
};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{
    DefSubclassProc, ITaskbarList3, SetWindowSubclass, THBF_ENABLED, THB_FLAGS, THB_ICON,
    THB_TOOLTIP, THUMBBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, GetClientRect, RegisterWindowMessageW, HICON, ICONINFO,
    WM_SETTINGCHANGE,
};

/// CLSID_TaskbarList（{56FDF344-FD6D-11d0-958A-006097C9A090}），windows 0.61 未导出该常量
const CLSID_TASKBARLIST: windows::core::GUID =
    windows::core::GUID::from_u128(0x56FDF344_FD6D_11D0_958A_006097C9A090);

/// 按钮命令 id（WM_COMMAND LOWORD(wParam)）
const BTN_PREV: u32 = 1001;
const BTN_TOGGLE: u32 = 1002;
const BTN_NEXT: u32 = 1003;
const SUBCLASS_ID: usize = 0x1A4D;
/// Win32 WM_COMMAND
const WM_COMMAND: u32 = 0x0111;
/// DWM 私有消息：请求窗口提供「图标式缩略图」（悬停任务栏按钮时）。
/// lParam：HIWORD = 允许的最大宽度，LOWORD = 允许的最大高度，位图超限会被 DWM 拒绝
const WM_DWMSENDICONICTHUMBNAIL: u32 = 0x0323;
/// DWM 私有消息：请求提供 Aero Peek / Alt+Tab 的整窗 Live Preview 位图（全尺寸）
const WM_DWMSENDICONICLIVEPREVIEWBITMAP: u32 = 0x0326;
/// 封面解码后保留的最大边长（与 covers.rs 的缓存规格一致）
const COVER_DECODE_MAX: u32 = 512;
/// 单次提交给 DWM 的位图长边上限：超出时等比缩到该值，
/// 防止高分屏大窗口按 1:1 合成位图造成不必要的内存与耗时
const PREVIEW_MAX_SIDE: u32 = 2048;

struct State {
    app: AppHandle,
    hwnd: HWND,
    taskbar: ITaskbarList3,
    /// 图标生成时的像素尺寸（DPI 缩放结果），主题切换重建图标时复用
    size: i32,
    icon_prev: HICON,
    icon_play: HICON,
    icon_pause: HICON,
    icon_next: HICON,
    playing: AtomicBool,
    // ---- DWM 图标式缩略图（任务栏悬停预览 = 当前歌曲封面）----
    /// 当前曲目所属专辑（前端 set_thumbbar_album 推送；None = 无曲目/无专辑）
    album: Option<i64>,
    /// 封面加载代数：每次切歌自增，用于丢弃过期的加载结果
    cover_gen: u64,
    /// 已解码封面（RGBA，≤512）。仅封面就绪时才开启 iconic 预览；
    /// 无封面时保持系统默认的真实窗口截图预览
    cover: Option<RgbaImage>,
    /// DWM 最近一次请求过的缩略图尺寸：封面切换完成后据此主动补刷一帧
    thumb_size: Option<(u32, u32)>,
}

/// 图标句柄在主题切换重建时需要整体替换，故用 Mutex 提供可变访问；
/// 所有实际使用仍收敛在主线程（跨线程仅读写 playing 原子量）。
static STATE: OnceLock<std::sync::Mutex<State>> = OnceLock::new();
/// RegisterWindowMessage("TaskbarButtonCreated") 的消息 id
static TASKBAR_CREATED_MSG: OnceLock<u32> = OnceLock::new();

// HWND / HICON / COM 接口指针在 Win32 语义下本就不受 Rust 线程约束；
// 实际使用：COM 对象与 UI 操作全部收敛在主线程，跨线程仅读写 playing 原子量。
unsafe impl Send for State {}
unsafe impl Sync for State {}

/// 在 setup 阶段（主线程）调用：初始化图标与 COM 对象，安装窗口子类化，并启动兜底重试。
pub fn init(app: AppHandle, hwnd: HWND) {
    unsafe {
        // 注册任务栏按钮创建广播消息（子类化过程据此重新挂按钮）
        let _ = TASKBAR_CREATED_MSG.set(RegisterWindowMessageW(w!("TaskbarButtonCreated")));

        // COM 可能已被 webview2 初始化为其他模式，失败不影响主流程（对象创建失败则放弃）
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let taskbar: ITaskbarList3 = match CoCreateInstance(&CLSID_TASKBARLIST, None, CLSCTX_ALL) {
            Ok(t) => t,
            Err(_) => {
                eprintln!("thumbbar: 创建 ITaskbarList3 失败，任务栏控制按钮不可用");
                return;
            }
        };

        // 按窗口 DPI 缩放图标尺寸（16dp 基准，clamp 防御异常 DPI）
        let size = ((16 * GetDpiForWindow(hwnd)) / 96).clamp(16, 64) as i32;
        // 图标颜色跟随系统浅/深色主题：浅色主题的按钮底色是浅色，需用深色图标才可见
        let light_ui = uses_light_ui();
        let (Some(icon_prev), Some(icon_play), Some(icon_pause), Some(icon_next)) = (
            make_icon(IconKind::Prev, size, light_ui),
            make_icon(IconKind::Play, size, light_ui),
            make_icon(IconKind::Pause, size, light_ui),
            make_icon(IconKind::Next, size, light_ui),
        ) else {
            eprintln!("thumbbar: 生成按钮图标失败，任务栏控制按钮不可用");
            return;
        };

        let state = State {
            app,
            hwnd,
            taskbar,
            size,
            icon_prev,
            icon_play,
            icon_pause,
            icon_next,
            playing: AtomicBool::new(false),
            album: None,
            cover_gen: 0,
            cover: None,
            thumb_size: None,
        };
        if STATE.set(std::sync::Mutex::new(state)).is_err() {
            return; // 仅主窗口，理论不会重复初始化
        }

        if SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0).as_bool() {
            // 兜底：启动广播可能在子类化安装前已发出（窗口默认可见），
            // 后台线程定期在主线程重试添加，成功即停。
            let app = match STATE.get() {
                Some(s) => s.lock().unwrap().app.clone(),
                None => return,
            };
            std::thread::spawn(move || {
                for _ in 0..20 {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let done = std::sync::Arc::new(AtomicBool::new(false));
                    let done2 = done.clone();
                    let h = app.clone();
                    let _ = h.run_on_main_thread(move || {
                        let ok = add_buttons().is_ok();
                        done2.store(ok, Ordering::SeqCst);
                    });
                    if done.load(Ordering::SeqCst) {
                        break;
                    }
                }
            });
        }
    }
}

/// 读取 HKCU Personalize 主题键下的 DWORD 值（regGetValueW 支持在 HKEY 上直接按值路径读取）
fn read_theme_dword<
    A: windows::core::Param<windows::core::PCWSTR>,
    B: windows::core::Param<windows::core::PCWSTR>,
>(
    subkey: A,
    value: B,
) -> Option<u32> {
    unsafe {
        let mut data: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        let err = RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut core::ffi::c_void),
            Some(&mut len),
        );
        (err == ERROR_SUCCESS).then_some(data)
    }
}

/// 判断当前是否浅色 UI（决定缩略图按钮图标颜色）。
/// 优先任务栏所属的系统模式（SystemUsesLightTheme，Win10 1903+ / Win11）；
/// 旧系统缺该键时回退应用模式（AppsUseLightTheme）；
/// 均读取失败时默认深色（白色图标，与历史行为一致）。
fn uses_light_ui() -> bool {
    let sub = w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    if let Some(v) = read_theme_dword(sub, w!("SystemUsesLightTheme")) {
        return v != 0;
    }
    if let Some(v) = read_theme_dword(sub, w!("AppsUseLightTheme")) {
        return v != 0;
    }
    false
}

/// 前端播放状态变化时同步中间按钮图标（播放中显示「暂停」图标）。
/// 可能在任意线程被调用（Tauri 命令线程池），实际 UI 操作转发到主线程。
pub fn set_playing(playing: bool) {
    let Some(m) = STATE.get() else { return };
    // 先原子记录播放状态，并取回 AppHandle 用于派发到主线程
    let app = {
        let s = m.lock().unwrap();
        s.playing.store(playing, Ordering::SeqCst);
        s.app.clone()
    };
    let _ = app.run_on_main_thread(move || unsafe {
        let Some(m) = STATE.get() else { return };
        let s = m.lock().unwrap();
        let icon = if s.playing.load(Ordering::SeqCst) {
            s.icon_pause
        } else {
            s.icon_play
        };
        let btn = thumb_button(BTN_TOGGLE, icon, "播放 / 暂停");
        let _ = s.taskbar.ThumbBarUpdateButtons(s.hwnd, &[btn]);
    });
}

unsafe fn add_buttons() -> windows::core::Result<()> {
    let Some(m) = STATE.get() else {
        return Err(windows::core::Error::from_win32());
    };
    let s = m.lock().unwrap();
    s.taskbar.HrInit()?;
    let playing = s.playing.load(Ordering::SeqCst);
    let buttons = [
        thumb_button(BTN_PREV, s.icon_prev, "上一首"),
        thumb_button(
            BTN_TOGGLE,
            if playing { s.icon_pause } else { s.icon_play },
            "播放 / 暂停",
        ),
        thumb_button(BTN_NEXT, s.icon_next, "下一首"),
    ];
    s.taskbar.ThumbBarAddButtons(s.hwnd, &buttons)
}

fn thumb_button(id: u32, icon: HICON, tip: &str) -> THUMBBUTTON {
    let mut sz_tip = [0u16; 260];
    for (i, u) in tip.encode_utf16().take(259).enumerate() {
        sz_tip[i] = u;
    }
    THUMBBUTTON {
        dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS,
        iId: id,
        iBitmap: 0,
        hIcon: icon,
        szTip: sz_tip,
        dwFlags: THBF_ENABLED,
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uid: usize,
    _data: usize,
) -> LRESULT {
    if Some(&msg) == TASKBAR_CREATED_MSG.get() {
        // 任务栏按钮（重新）创建：重新挂按钮（explorer 重启后也会收到）
        let _ = add_buttons();
        return DefSubclassProc(hwnd, msg, wparam, lparam);
    }
    if msg == WM_DWMSENDICONICTHUMBNAIL {
        // DWM 索要悬停缩略图：提交当前歌曲封面（仅开启 iconic 预览后会收到）
        on_iconic_thumbnail_request(hwnd, lparam);
        return LRESULT(0);
    }
    if msg == WM_DWMSENDICONICLIVEPREVIEWBITMAP {
        // DWM 索要 Aero Peek / Alt+Tab 整窗预览：同样整块显示封面
        on_iconic_live_preview_request(hwnd);
        return LRESULT(0);
    }
    if msg == WM_COMMAND {
        let action = match (wparam.0 & 0xffff) as u32 {
            BTN_PREV => Some("prev"),
            BTN_TOGGLE => Some("toggle"),
            BTN_NEXT => Some("next"),
            _ => None,
        };
        if let Some(action) = action {
            if let Some(m) = STATE.get() {
                let s = m.lock().unwrap();
                let _ = s.app.emit("tray", action);
            }
        }
    } else if msg == WM_SETTINGCHANGE && immersive_color_set(lparam) {
        // 系统浅/深色主题切换广播：重建图标颜色并刷新按钮
        rebuild_icons_on_theme_change();
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// WM_SETTINGCHANGE 的 lParam 指向引起变化的设置项名（宽字符串），
/// 系统浅/深色模式切换时广播的字符串为 "ImmersiveColorSet"。
unsafe fn immersive_color_set(lparam: LPARAM) -> bool {
    if lparam.0 == 0 {
        return false;
    }
    let p = lparam.0 as *const u16;
    let needle = "ImmersiveColorSet";
    let mut i = 0usize;
    for wc in needle.encode_utf16() {
        // 字符串在匹配到完整名称前结束则不是目标广播
        if *p.add(i) != wc {
            return false;
        }
        i += 1;
    }
    // 名称需在此结束，排除 "ImmersiveColorSet..." 之类的前缀
    *p.add(i) == 0
}

/// 系统主题切换时在主线程重建四个按钮图标：按新主题重绘，
/// 刷新成功后销毁旧图标并替换（失败则保留旧图标继续生效）。
unsafe fn rebuild_icons_on_theme_change() {
    let Some(m) = STATE.get() else { return };
    let mut s = m.lock().unwrap();
    let light_ui = uses_light_ui();
    let playing = s.playing.load(Ordering::SeqCst);
    let icons = [
        make_icon(IconKind::Prev, s.size, light_ui),
        make_icon(IconKind::Play, s.size, light_ui),
        make_icon(IconKind::Pause, s.size, light_ui),
        make_icon(IconKind::Next, s.size, light_ui),
    ];
    // 任一生成失败则整体放弃（保留旧图标），并释放已成功的句柄避免泄漏
    if icons.iter().any(|h| h.is_none()) {
        for h in icons.into_iter().flatten() {
            let _ = DestroyIcon(h);
        }
        return;
    }
    let [h_prev, h_play, h_pause, h_next] = icons.map(Option::unwrap);
    let buttons = [
        thumb_button(BTN_PREV, h_prev, "上一首"),
        thumb_button(
            BTN_TOGGLE,
            if playing { h_pause } else { h_play },
            "播放 / 暂停",
        ),
        thumb_button(BTN_NEXT, h_next, "下一首"),
    ];
    if s.taskbar.ThumbBarUpdateButtons(s.hwnd, &buttons).is_ok() {
        // 任务栏已持有新图标，替换 State 后再销毁旧句柄
        let old = (s.icon_prev, s.icon_play, s.icon_pause, s.icon_next);
        s.icon_prev = h_prev;
        s.icon_play = h_play;
        s.icon_pause = h_pause;
        s.icon_next = h_next;
        drop(s);
        for h in [old.0, old.1, old.2, old.3] {
            let _ = DestroyIcon(h);
        }
    } else {
        drop(s);
        for h in [h_prev, h_play, h_pause, h_next] {
            let _ = DestroyIcon(h);
        }
    }
}

// ---------- 图标绘制 ----------

#[derive(Clone, Copy)]
enum IconKind {
    Prev,
    Play,
    Pause,
    Next,
}

/// 用代码绘制形状（浅色主题用深色前景、深色主题用白色）并生成带 alpha 通道的 HICON（背景透明）。
unsafe fn make_icon(kind: IconKind, size: i32, light_ui: bool) -> Option<HICON> {
    // 浅色主题按钮底色浅 → 用柔和深灰前景（纯黑太突兀）；深色主题按钮底色深 → 白色前景
    let fg = if light_ui {
        (0x4D, 0x4D, 0x4D)
    } else {
        (255, 255, 255)
    };
    let rgba = render(kind, size, fg);

    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size, // 负值 = 自上而下
            biPlanes: 1,
            biBitCount: 32,
            ..Default::default()
        },
        ..Default::default()
    };
    let hbmp = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
    std::ptr::copy_nonoverlapping(rgba.as_ptr(), bits.cast::<u8>(), rgba.len());

    // 单色掩码：32bpp 位图带 alpha 时掩码被忽略，但结构上必须提供
    let mask = CreateBitmap(size, size, 1, 1, None);
    let info = ICONINFO {
        fIcon: true.into(),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: hbmp,
    };
    let icon = CreateIconIndirect(&info).ok();
    let _ = DeleteObject(hbmp.into());
    let _ = DeleteObject(mask.into());
    icon
}

/// 按归一化坐标绘制各形状。`fg` 为前景 RGB（图形不透明色，其余全透明）。
fn render(kind: IconKind, size: i32, fg: (u8, u8, u8)) -> Vec<u8> {
    let s = size as f32;
    let mut buf = vec![0u8; (size * size) as usize * 4];
    for y in 0..size {
        for x in 0..size {
            let px = (x as f32 + 0.5) / s;
            let py = (y as f32 + 0.5) / s;
            let on = match kind {
                IconKind::Play => in_tri(px, py, (0.22, 0.14), (0.22, 0.86), (0.82, 0.50)),
                IconKind::Pause => {
                    in_rect(px, py, 0.26, 0.14, 0.40, 0.86)
                        || in_rect(px, py, 0.60, 0.14, 0.74, 0.86)
                }
                // 上一首：右缘竖条 + 左向三角；下一首：左缘竖条 + 右向三角
                IconKind::Next => {
                    in_rect(px, py, 0.76, 0.14, 0.88, 0.86)
                        || in_tri(px, py, (0.14, 0.14), (0.14, 0.86), (0.70, 0.50))
                }
                IconKind::Prev => {
                    in_rect(px, py, 0.12, 0.14, 0.24, 0.86)
                        || in_tri(px, py, (0.86, 0.14), (0.86, 0.86), (0.30, 0.50))
                }
            };
            if on {
                let i = ((y * size + x) * 4) as usize;
                buf[i] = fg.0;
                buf[i + 1] = fg.1;
                buf[i + 2] = fg.2;
                buf[i + 3] = 255;
            }
        }
    }
    buf
}

fn in_rect(px: f32, py: f32, l: f32, t: f32, r: f32, b: f32) -> bool {
    px >= l && px <= r && py >= t && py <= b
}

/// 点是否在三角形内（半平面符号一致性判定）
fn in_tri(px: f32, py: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    fn sign(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
        (p.0 - b.0) * (a.1 - b.1) - (a.0 - b.0) * (p.1 - b.1)
    }
    let d1 = sign((px, py), a, b);
    let d2 = sign((px, py), b, c);
    let d3 = sign((px, py), c, a);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

// ---------- DWM 图标式缩略图：悬停预览整块显示当前歌曲封面 ----------

/// 前端切歌时报告当前专辑（任意线程可调用）。
///
/// 封面提取可能触发磁盘/网络读取，放在后台线程执行：就绪前先关闭 iconic 预览，
/// 让缩略图回退为真实窗口内容（避免显示上一首的过期封面）；
/// 封面解码成功后开启 iconic 预览，下一次 DWM 请求时即整块显示新封面。
pub fn set_album(album_id: Option<i64>) {
    let Some(m) = STATE.get() else { return };
    let (app, gen) = {
        let mut s = m.lock().unwrap();
        if s.album == album_id {
            return; // 同专辑重复推送（如队列内连播同一专辑），无需重复加载
        }
        s.album = album_id;
        s.cover_gen += 1;
        let gen = s.cover_gen;
        s.cover = None;
        s.thumb_size = None;
        (s.app.clone(), gen)
    };
    // 先关闭 iconic 预览并作废旧位图，避免缩略图继续显示上一首封面
    let _ = app.run_on_main_thread(move || unsafe {
        if let Some(m) = STATE.get() {
            let s = m.lock().unwrap();
            set_iconic_attrs(s.hwnd, false);
            let _ = DwmInvalidateIconicBitmaps(s.hwnd);
        }
    });

    let done = app.clone();
    std::thread::spawn(move || {
        // 后台提取封面（ensure_cover 走缓存文件；缺失时可能回源到本地标签 / WebDAV）
        let decoded: Option<RgbaImage> = album_id.and_then(|id| {
            let path = crate::covers::ensure_cover(&app, id).ok().flatten()?;
            let bytes = std::fs::read(&path).ok()?;
            decode_cover_rgba(&bytes)
        });
        let _ = done.run_on_main_thread(move || unsafe {
            let Some(m) = STATE.get() else { return };
            let (hwnd, thumb_size) = {
                let mut s = m.lock().unwrap();
                if s.cover_gen != gen {
                    return; // 加载期间又切了歌，丢弃过期结果
                }
                s.cover = decoded;
                // 只有真取到封面才开启封面预览，否则保持系统默认窗口预览
                set_iconic_attrs(s.hwnd, s.cover.is_some());
                let _ = DwmInvalidateIconicBitmaps(s.hwnd);
                (s.hwnd, s.thumb_size)
            };
            // 若用户正悬停在缩略图上，按最近请求过的尺寸立即补刷，无需等下一次请求
            if let Some((w, h)) = thumb_size {
                submit_iconic_bitmap(hwnd, w, h, false);
            }
        });
    });
}

/// 开关 DWM「图标式缩略图」：开启后任务栏缩略图 / Peek / Alt+Tab 一律显示程序提交的
/// 封面位图；关闭则回退为系统抓取窗口画面的默认预览。
unsafe fn set_iconic_attrs(hwnd: HWND, on: bool) {
    // 两个属性都取 BOOL（i32）值
    let v: i32 = if on { 1 } else { 0 };
    let pv = &v as *const i32 as *const core::ffi::c_void;
    let cb = std::mem::size_of::<i32>() as u32;
    let _ = DwmSetWindowAttribute(hwnd, DWMWA_HAS_ICONIC_BITMAP, pv, cb);
    let _ = DwmSetWindowAttribute(hwnd, DWMWA_FORCE_ICONIC_REPRESENTATION, pv, cb);
}

/// 响应 DWM 的缩略图请求：lParam 的 HIWORD / LOWORD 是允许的最大宽 / 高，
/// 提交的位图任一方超过都会被 DWM 拒绝，因此严格按该尺寸渲染封面。
unsafe fn on_iconic_thumbnail_request(hwnd: HWND, lparam: LPARAM) {
    let max_w = ((lparam.0 >> 16) & 0xffff) as u32;
    let max_h = (lparam.0 & 0xffff) as u32;
    if max_w == 0 || max_h == 0 {
        return;
    }
    if let Some(m) = STATE.get() {
        // 记住尺寸：封面切换完成后可主动补刷一帧，不必等下次悬停
        m.lock().unwrap().thumb_size = Some((max_w, max_h));
    }
    submit_iconic_bitmap(hwnd, max_w, max_h, false);
}

/// 响应 DWM 的整窗 Live Preview 请求（Aero Peek / Alt+Tab）：
/// 位图规格为窗口客户区大小，用封面按客户区比例整块铺满。
unsafe fn on_iconic_live_preview_request(hwnd: HWND) {
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if GetClientRect(hwnd, &mut rc).is_err() {
        return;
    }
    let w = (rc.right - rc.left).max(1) as u32;
    let h = (rc.bottom - rc.top).max(1) as u32;
    submit_iconic_bitmap(hwnd, w, h, true);
}

/// 把当前封面按 cover 语义铺满 w×h（居中裁剪、保持宽高比）合成 32bpp 位图提交给 DWM。
/// 封面未就绪时直接返回（DWM 会显示自己的默认内容）。
unsafe fn submit_iconic_bitmap(hwnd: HWND, w: u32, h: u32, live_preview: bool) {
    // 先复制封面再渲染，避免渲染期间长时间占用 STATE 锁（阻塞其它线程的按钮同步）
    let Some(src) = STATE.get().and_then(|m| m.lock().unwrap().cover.clone()) else {
        return;
    };
    let (w, h) = clamp_preview_size(w, h);
    let Some(out) = cover_fill(&src, w, h) else {
        return;
    };
    let (w, h) = out.dimensions();

    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32), // 负值 = 自上而下
            biPlanes: 1,
            biBitCount: 32,
            ..Default::default()
        },
        ..Default::default()
    };
    let Ok(hbmp) = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
        return;
    };
    // 32bpp DIB 内存按 B,G,R,A 排列，而 RgbaImage 是 R,G,B,A：写入时交换 R/B
    let dst = std::slice::from_raw_parts_mut(bits.cast::<u8>(), (w * h) as usize * 4);
    for (chunk, px) in dst.as_chunks_mut::<4>().0.iter_mut().zip(out.pixels()) {
        chunk[0] = px[2];
        chunk[1] = px[1];
        chunk[2] = px[0];
        chunk[3] = 255;
    }
    let result = if live_preview {
        DwmSetIconicLivePreviewBitmap(hwnd, hbmp, None, DWM_SIT_DISPLAYFRAME)
    } else {
        DwmSetIconicThumbnail(hwnd, hbmp, 0)
    };
    let _ = result;
    // DWM 在内部持有位图副本，提交完成后即可释放句柄
    let _ = DeleteObject(hbmp.into());
}

/// 解码封面并等比缩到 ≤512（缓存文件通常已是该规格，这里防御超大原图）
fn decode_cover_rgba(bytes: &[u8]) -> Option<RgbaImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w <= COVER_DECODE_MAX && h <= COVER_DECODE_MAX {
        return Some(rgba);
    }
    let k = COVER_DECODE_MAX as f32 / w.max(h) as f32;
    let nw = ((w as f32) * k).round().max(1.0) as u32;
    let nh = ((h as f32) * k).round().max(1.0) as u32;
    Some(image::imageops::resize(
        &rgba,
        nw,
        nh,
        image::imageops::FilterType::Lanczos3,
    ))
}

/// 等比限制渲染尺寸到上限内，防止超大窗口按 1:1 合成位图
fn clamp_preview_size(w: u32, h: u32) -> (u32, u32) {
    let m = w.max(h).max(1) as f32;
    let k = (PREVIEW_MAX_SIDE as f32 / m).min(1.0);
    (
        ((w as f32) * k).round().max(1.0) as u32,
        ((h as f32) * k).round().max(1.0) as u32,
    )
}

/// 以 CSS `background-size: cover` 语义把封面居中裁剪铺满 w×h：
/// 保持宽高比不变，等比放大后裁掉溢出部分。
fn cover_fill(src: &RgbaImage, w: u32, h: u32) -> Option<RgbaImage> {
    let (sw, sh) = src.dimensions();
    if sw == 0 || sh == 0 || w == 0 || h == 0 {
        return None;
    }
    let src_ar = sw as f32 / sh as f32;
    let dst_ar = w as f32 / h as f32;
    // 从源图裁出与目标同宽高比的居中区域
    let (cw, ch, ox, oy) = if src_ar > dst_ar {
        // 源图更宽：保留全高，裁剪左右
        let cw = ((sh as f32) * dst_ar).round().clamp(1.0, sw as f32) as u32;
        (cw, sh, (sw - cw) / 2, 0)
    } else {
        // 源图更高或同比例：保留全宽，裁剪上下
        let ch = ((sw as f32) / dst_ar).round().clamp(1.0, sh as f32) as u32;
        (sw, ch, 0, (sh - ch) / 2)
    };
    let crop = image::imageops::crop_imm(src, ox, oy, cw, ch).to_image();
    Some(image::imageops::resize(
        &crop,
        w,
        h,
        image::imageops::FilterType::Triangle,
    ))
}
