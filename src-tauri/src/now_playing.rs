//! 系统级「正在播放」：Windows SMTC / macOS Now Playing / Linux MPRIS（souvlaki 实现）。
//!
//! 与系统之间双向各一条路：
//! - **出（应用 → 系统）**：前端在切歌、播放暂停、seek 与 1Hz 心跳时经 IPC 推送元数据与
//!   进度（见 `useNowPlaying.ts`），这里转成 souvlaki 的 `set_metadata` / `set_playback`，
//!   系统媒体浮层、锁屏、外设屏据此显示；
//! - **入（系统 → 应用）**：`attach` 的回调里把播放控制类事件统一以 `media-control` 事件
//!   广播给前端（payload `{action, positionMs?}`，与 `media_controls.rs` 的全局媒体键同一
//!   通道、同一消费者）；seek 类（SetPosition/Seek/SeekBy）换算成绝对位置后以
//!   `action=seekto` 广播；Raise 直接在 Rust 侧唤起主窗口。
//!
//! 实现要点（souvlaki 0.8.3 源码核对 + 实机回归）：
//! - Windows 经 interop `GetForWindow` 绑主窗口，**hwnd 缺失会在 souvlaki 内部 panic**
//!   （`new` 里直接 `expect`），因此初始化必须发生在主窗口创建之后；
//! - `set_metadata` 携带封面时在 Rust 侧**同步阻塞**等待文件异步加载（`loader.get()`），
//!   所以对外命令一律走 `spawn_blocking`；
//! - souvlaki 内部**缓存一份 timeline_properties**：`set_metadata` 会带着缓存里的旧
//!   Position + 新 EndTime 调 `UpdateTimelineProperties`，Position 越界即 E_INVALIDARG、
//!   元数据提交（`display_updater.Update()`）根本不会执行——**写元数据前必须先把缓存
//!   Position 归零**（归零动作本身处于「已在的状态」批次，安全）；
//! - 【Win11 实测怪癖】**暂停这类纯状态切换绝不能连带时间轴一起推**：浮层处理 `Paused`
//!   状态的途中再收 `UpdateTimelineProperties` 会丢掉已提交的元数据，卡片塌缩成
//!   「应用名+按钮」且恢复播放也不回来（元数据是真丢了）。对齐 Chromium
//!   `system_media_controls_win` 的行为：暂停只发 `SetPlaybackStatus`，时间轴仅随元数据
//!   与 seek 更新。旁路用项目自己的 windows crate 经 interop 再取同一窗口的 SMTC 实例
//!   （窗口级单例，与 souvlaki 持有的是同一个对象），只调状态；
//! - 封面 URL 三平台统一 `file://` + `Path::display()` 原样拼接：Windows trim 后剩 `C:\...`
//!   （`GetFileFromPathAsync` 可解析），Unix 剩 `/...` 正好构成 `file:///...`；
//!   `cover://` 自定义协议在这里不可用（souvlaki 不认识）；
//! - 封面来源由命令层解析（commands.rs::now_playing_set）：缓存命中直接用；未命中先推
//!   **应用图标占位**（souvlaki 不带封面时**不清系统侧缩略图**，上一首的封面会一直挂着，
//!   所以「无封面」必须显式给占位图）再后台提取，成功且仍是当前曲目时经
//!   `refresh_cover` 补推（按 last_album_id 校验，切歌后到期的补推作废）；
//! - `MediaControls` **drop 即与系统解除关联**，必须保存在静态里；
//! - macOS 后端无状态（全局单例式 MPNowPlayingInfoCenter/MPRemoteCommandCenter），
//!   初始化放在 setup（主线程）即可。

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use serde_json::json;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::NpMeta;

/// 静态状态：MediaControls 保活 + 记住最近一次元数据 / 进度，
/// 供开关重开时恢复显示、以及相对 seek（±10s）换算绝对位置
struct NpState {
    controls: Option<MediaControls>,
    last_meta: Option<LastMeta>,
    /// 当前曲目专辑 id：后台封面补推（refresh_cover）的新鲜度校验用，
    /// 切歌后到期的补推以此判定作废
    last_album_id: Option<i64>,
    position_ms: u64,
    duration_ms: u64,
    playing: bool,
    /// 本轮暂停期内「对账重发」已执行次数：首次重发 INFO 留痕，之后降 debug
    /// （挂机暂停每 30s 一条纯噪音）。播放恢复/暂停瞬间清零，开新一轮周期。
    paused_reissue: u32,
    /// 主窗口 HWND（仅 Windows）：纯状态旁路经 interop 再取同一 SMTC 实例用
    #[cfg(windows)]
    hwnd_usize: Option<usize>,
}

#[derive(Clone)]
struct LastMeta {
    title: String,
    artist: Option<String>,
    album: Option<String>,
    duration_ms: Option<u64>,
    cover_url: Option<String>,
}

static NP: OnceLock<Mutex<NpState>> = OnceLock::new();

fn lock() -> Result<MutexGuard<'static, NpState>, String> {
    NP.get_or_init(|| {
        Mutex::new(NpState {
            controls: None,
            last_meta: None,
            last_album_id: None,
            position_ms: 0,
            duration_ms: 0,
            playing: false,
            paused_reissue: 0,
            #[cfg(windows)]
            hwnd_usize: None,
        })
    })
    .lock()
    .map_err(|_| "now_playing 状态锁中毒".into())
}

/// 应用启动时调用（setup 内、主窗口已创建）：默认启用系统媒体控件。
/// 用户主动关闭过的（`lm.nowPlaying='0'`），前端启动对账时会调 `now_playing_enable(false)` 关掉。
pub fn init(app: &AppHandle) {
    if let Err(e) = enable(app, true) {
        log::warn!("系统媒体控件初始化失败（本次会话不再重试）: {e}");
    }
}

/// 启用 / 禁用系统媒体控件（设置开关调用）。禁用即与系统解除关联；
/// 重新启用会用记住的元数据与进度恢复显示。
pub fn enable(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if !enabled {
        let mut guard = lock()?;
        guard.controls = None; // drop 即 detach
        guard.playing = false;
        #[cfg(windows)]
        {
            guard.hwnd_usize = None;
        }
        return Ok(());
    }

    // Windows 必须有主窗口 hwnd（缺失会在 souvlaki 内部 panic，见模块注释）
    #[cfg(windows)]
    let (hwnd, hwnd_usize) = match app.get_webview_window("main") {
        Some(w) => match w.hwnd() {
            Ok(h) => {
                let p = h.0 as usize;
                (Some(h.0), Some(p))
            }
            Err(e) => return Err(format!("获取主窗口 HWND 失败: {e}")),
        },
        None => return Err("主窗口尚未创建".into()),
    };
    #[cfg(not(windows))]
    let hwnd = None;

    let mut controls = MediaControls::new(PlatformConfig {
        display_name: "LanMusic",
        dbus_name: "com.lanmusic.desktop",
        hwnd,
    })
    .map_err(|e| format!("创建系统媒体控件失败: {e}"))?;

    let handle = app.clone();
    controls
        .attach(move |event| handle_event(&handle, event))
        .map_err(|e| format!("挂接系统媒体控件事件失败: {e}"))?;

    let mut guard = lock()?;
    guard.controls = Some(controls);
    #[cfg(windows)]
    {
        guard.hwnd_usize = hwnd_usize;
    }
    // 恢复最近一次元数据与播放状态：重新开启时系统浮层不留空白
    let (last, playing, position_ms) = {
        let last = guard.last_meta.clone();
        let playing = guard.playing;
        let position_ms = guard.position_ms;
        (last, playing, position_ms)
    };
    if let Some(m) = &last {
        // 新建的 MediaControls 内部时间轴缓存为默认值，走切歌同款归零路径最稳；
        // 紧随其后的 apply_playback 会把真实进度与播放状态写回
        let r = apply_metadata(&mut guard, m, true);
        if let Err(e) = &r {
            log::warn!("系统媒体控件恢复元数据失败: {e}");
        }
    }
    let r = apply_playback(&mut guard, playing, position_ms);
    if let Err(e) = &r {
        log::warn!("系统媒体控件恢复播放状态失败: {e}");
    }
    Ok(())
}

fn handle_event(app: &AppHandle, event: MediaControlEvent) {
    // 播放控制类：与全局媒体键同通道，前端 useMediaControls.ts 统一消费
    let action = match &event {
        MediaControlEvent::Play => Some("play"),
        MediaControlEvent::Pause => Some("pause"),
        MediaControlEvent::Toggle => Some("playpause"),
        MediaControlEvent::Next => Some("next"),
        MediaControlEvent::Previous => Some("prev"),
        MediaControlEvent::Stop => Some("stop"),
        _ => None,
    };
    if let Some(a) = action {
        let _ = app.emit("media-control", json!({ "action": a }));
        return;
    }
    match event {
        MediaControlEvent::SetPosition(p) => emit_seek(app, p.0.as_millis() as u64),
        MediaControlEvent::Seek(dir) => seek_relative(dir, 10_000, app),
        MediaControlEvent::SeekBy(dir, d) => seek_relative(dir, d.as_millis() as u64, app),
        MediaControlEvent::Raise => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }
        // SetVolume / OpenUri / Quit：不做（音量跟随系统；退出交给用户自己）
        _ => {}
    }
}

/// 相对 seek（系统侧「快进 / 快退」按钮，默认 ±10s）：换算成绝对位置广播
fn seek_relative(dir: SeekDirection, delta_ms: u64, app: &AppHandle) {
    let (position_ms, duration_ms) = match lock() {
        Ok(g) => (g.position_ms, g.duration_ms),
        Err(_) => return,
    };
    let target = match dir {
        SeekDirection::Forward => position_ms.saturating_add(delta_ms),
        SeekDirection::Backward => position_ms.saturating_sub(delta_ms),
    };
    let target = if duration_ms > 0 {
        target.min(duration_ms)
    } else {
        target
    };
    emit_seek(app, target);
}

fn emit_seek(app: &AppHandle, position_ms: u64) {
    let _ = app.emit(
        "media-control",
        json!({ "action": "seekto", "positionMs": position_ms }),
    );
}

/// 写入元数据（meta=None 表示清空 → Stopped）；cover_url 由命令层解析好传入。
/// 切歌路径：进度归零（前端 load 时已归零，随后的心跳立刻推真实进度）。
pub fn set_metadata(meta: Option<NpMeta>, cover_url: Option<String>) -> Result<(), String> {
    let mut guard = lock()?;
    let Some(m) = meta else {
        guard.last_meta = None;
        guard.last_album_id = None;
        guard.playing = false;
        guard.position_ms = 0;
        guard.duration_ms = 0;
        if let Some(c) = guard.controls.as_mut() {
            c.set_playback(MediaPlayback::Stopped)
                .map_err(|e| e.to_string())?;
        }
        return Ok(());
    };
    apply_new_meta(&mut guard, m, cover_url, true)
}

/// 后台封面提取完成后的补推（修「切歌后浮层一直显示上一首封面」）：
/// 只有系统侧当前仍是这首歌时才生效（用户已切歌则静默丢弃）。
/// 同曲补推不重置进度、不发归零批次（时间轴缓存就是当前真实位置，对新 EndTime 合法）。
pub fn refresh_cover(album_id: i64, meta: NpMeta, cover_url: Option<String>) -> Result<(), String> {
    let mut guard = lock()?;
    if guard.last_album_id != Some(album_id) {
        return Ok(()); // 已切歌，到期的补推作废
    }
    apply_new_meta(&mut guard, meta, cover_url, false)
}

/// 元数据推送公共路径。`reset_position`：
/// - `true`（切歌）：我方进度归零 + souvlaki 时间轴缓存归零（上一首的旧位置对新
///   EndTime 会越界 → E_INVALIDARG，见模块注释）；
/// - `false`（同曲补封面）：两者都不动，避免暂停态下的多余时间轴推送。
fn apply_new_meta(
    guard: &mut NpState,
    m: NpMeta,
    cover_url: Option<String>,
    reset_position: bool,
) -> Result<(), String> {
    guard.last_album_id = m.album_id;
    let last = LastMeta {
        title: m.title,
        artist: m.artist,
        album: m.album,
        duration_ms: m.duration_ms,
        cover_url,
    };
    guard.duration_ms = last.duration_ms.unwrap_or(0);
    if reset_position {
        guard.position_ms = 0;
    }
    let r = apply_metadata(guard, &last, reset_position);
    if let Err(e) = &r {
        log::warn!("推送曲目元数据到系统媒体控件失败: {e}");
    }
    if r.is_ok() {
        guard.last_meta = Some(last);
    }
    r
}

fn apply_metadata(guard: &mut NpState, meta: &LastMeta, zero_first: bool) -> Result<(), String> {
    let Some(c) = guard.controls.as_mut() else {
        return Ok(()); // 未启用（或已被禁用）：只记元数据，重开时再恢复
    };
    let duration = meta
        .duration_ms
        .filter(|ms| *ms > 0)
        .map(Duration::from_millis);
    let md = MediaMetadata {
        title: Some(&meta.title),
        artist: meta.artist.as_deref(),
        album: meta.album.as_deref(),
        duration,
        cover_url: meta.cover_url.as_deref(),
    };
    // zero_first（切歌路径）：先把 souvlaki 内部缓存的 timeline Position 归零（见模块
    // 注释）。此批推送的状态与当前状态同值，且不处于「状态切换瞬间」。
    // 同曲补封面（zero_first=false）不需要：缓存位置就是当前真实位置，对新 EndTime
    // 合法；跳过归零批次也避免暂停态下的多余时间轴推送。
    if zero_first {
        c.set_playback(playback_of(guard.playing, 0, 0))
            .map_err(|e| format!("归零系统侧时间轴失败: {e}"))?;
    }
    c.set_metadata(md).map_err(|e| e.to_string())?;
    // set_metadata 内部已把时间轴写成「缓存位置 + 新 EndTime」：
    // - 切歌路径（缓存已被归零）→ 时间轴在 0 处，需把真实进度写回
    //   （切歌时进度本就从 0 开始，恢复路径 > 0）；
    // - 补封面路径（缓存即真实位置）→ 时间轴已正确，无需再推。
    if zero_first && guard.position_ms > 0 {
        let (playing, position_ms, duration_ms) =
            (guard.playing, guard.position_ms, guard.duration_ms);
        c.set_playback(playback_of(playing, position_ms, duration_ms))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 更新播放状态与进度（前端 1Hz 心跳 + 播放暂停 / seek 即时推送）
///
/// 推送规则（对齐 Chromium `system_media_controls_win`，规避 Win11 卡片塌缩怪癖）：
/// - 暂停瞬间（Playing→Paused）：**只发状态，绝不动时间轴**（Windows 走 `status_only`
///   旁路；其他平台无此怪癖，走 souvlaki 完整推送）；
/// - 已暂停时的心跳：位置冻结无新信息，直接跳过；
/// - 暂停中的 seek：位置变化才连同时间轴一起推（对齐 Chromium 的 SetPosition，
///   非切换批次、安全）；
/// - 播放中的心跳 / 恢复播放：完整推送（状态+时间轴）。1Hz 推送对卡片无害（实测），
///   且让非 1x 倍速下系统侧进度保持诚实。
pub fn set_state(playing: bool, position_ms: u64) -> Result<(), String> {
    let mut guard = lock()?;
    let r = apply_playback(&mut guard, playing, position_ms);
    if let Err(e) = &r {
        log::warn!("推送播放状态到系统媒体控件失败: {e}");
    }
    r
}

fn apply_playback(guard: &mut NpState, playing: bool, position_ms: u64) -> Result<(), String> {
    let was_playing = guard.playing;
    let old_pos = guard.position_ms;
    guard.playing = playing;
    guard.position_ms = position_ms;
    if playing || was_playing {
        // 播放中或暂停瞬间：非「暂停中对账」路径，重发计数清零开新一轮周期
        guard.paused_reissue = 0;
    }
    if guard.last_meta.is_none() || guard.controls.is_none() {
        return Ok(());
    }
    if !playing {
        if was_playing {
            // 暂停瞬间：只发状态。时间轴停在最后一次推送处（≤1s 误差），系统按暂停
            // 冻结进度条，恢复播放后下一次完整推送立即重同步。
            if let Some(r) = status_only(guard, false) {
                return r.inspect_err(|e| log::warn!("推送播放状态到系统媒体控件失败: {e}"));
            }
        } else if position_ms == old_pos {
            // 暂停中的对账（前端暂停期每 ≥30s 重发一次）：位置没动，但状态要重申——
            // 后台节流下「暂停瞬间的那次推送」存在丢失窗口，系统侧可能停在 Playing。
            // SetPlaybackStatus 幂等且不碰时间轴，重发对 Win11 塌缩怪癖安全。
            // 首次重发 INFO 留痕（证明对账在工作），之后降 debug：挂机时每 30s
            // 一条 INFO 是纯噪音。
            guard.paused_reissue += 1;
            if let Some(r) = status_only(guard, false) {
                return r
                    .inspect(|_| {
                        if guard.paused_reissue <= 1 {
                            log::info!("SMTC 状态对账重发: Paused");
                        } else {
                            log::debug!("SMTC 状态对账重发 x{}: Paused", guard.paused_reissue);
                        }
                    })
                    .inspect_err(|e| log::warn!("推送播放状态到系统媒体控件失败: {e}"));
            }
            // 旁路不可用（非 Windows）：无事可做
            return Ok(());
        }
        // 暂停中的 seek（位置变化）或旁路不可用（非 Windows）→ 走下方完整推送
    }
    let c = guard.controls.as_mut().expect("controls 已判空");
    c.set_playback(playback_of(playing, position_ms, guard.duration_ms))
        .map_err(|e| e.to_string())
}

/// 纯状态旁路（仅 Windows 有此怪癖）：返回 None 表示旁路不可用，调用方回落到
/// souvlaki 完整推送。
#[cfg(windows)]
fn status_only(guard: &NpState, playing: bool) -> Option<Result<(), String>> {
    Some(status_only_windows(guard.hwnd_usize?, playing))
}

#[cfg(not(windows))]
fn status_only(_guard: &NpState, _playing: bool) -> Option<Result<(), String>> {
    None
}

/// 用项目自己的 windows crate（0.61）经 interop 再取同一窗口的 SMTC 实例，
/// 只调 `SetPlaybackStatus`、完全不碰时间轴——规避「Paused+UpdateTimelineProperties
/// 同批到达导致浮层丢元数据」的 Win11 怪癖（见模块注释）。
#[cfg(windows)]
fn status_only_windows(hwnd_usize: usize, playing: bool) -> Result<(), String> {
    use windows::core::factory;
    use windows::Media::{MediaPlaybackStatus, SystemMediaTransportControls};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;

    let status = if playing {
        MediaPlaybackStatus::Playing
    } else {
        MediaPlaybackStatus::Paused
    };
    unsafe {
        let interop: ISystemMediaTransportControlsInterop =
            factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()
                .map_err(|e| format!("获取 SMTC interop 工厂失败: {e}"))?;
        let smtc: SystemMediaTransportControls = interop
            .GetForWindow(HWND(hwnd_usize as *mut std::ffi::c_void))
            .map_err(|e| format!("GetForWindow 失败: {e}"))?;
        smtc.SetPlaybackStatus(status)
            .map_err(|e| format!("SetPlaybackStatus 失败: {e}"))
    }
}

fn playback_of(playing: bool, position_ms: u64, duration_ms: u64) -> MediaPlayback {
    // Position 必须落在 [MinSeekTime, MaxSeekTime] 内（越界时
    // UpdateTimelineProperties 返回 E_INVALIDARG，见模块注释）；
    // 状态推送先于元数据到达、或曲目恰好播到末尾时都可能短暂越界，统一钳制。
    let position_ms = if duration_ms > 0 {
        position_ms.min(duration_ms)
    } else {
        position_ms
    };
    let progress = Some(MediaPosition(Duration::from_millis(position_ms)));
    if playing {
        MediaPlayback::Playing { progress }
    } else {
        MediaPlayback::Paused { progress }
    }
}
