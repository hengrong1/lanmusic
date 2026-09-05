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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use tauri::{AppHandle, Emitter};
use windows::core::w;
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND, LPARAM, LRESULT, WPARAM};
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
    CreateIconIndirect, DestroyIcon, RegisterWindowMessageW, WM_SETTINGCHANGE, HICON, ICONINFO,
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
            let _ = DestroyIcon(h.into());
        }
        return;
    }
    let [h_prev, h_play, h_pause, h_next] = icons.map(Option::unwrap);
    let buttons = [
        thumb_button(BTN_PREV, h_prev, "上一首"),
        thumb_button(BTN_TOGGLE, if playing { h_pause } else { h_play }, "播放 / 暂停"),
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
            let _ = DestroyIcon(h.into());
        }
    } else {
        drop(s);
        for h in [h_prev, h_play, h_pause, h_next] {
            let _ = DestroyIcon(h.into());
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
    let fg = if light_ui { (0x4D, 0x4D, 0x4D) } else { (255, 255, 255) };
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
