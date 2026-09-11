//! 用户可见错误的结构化编码（错误码契约）。
//!
//! 前端 i18n 翻不了 Rust 侧的文案，所以用户可见的错误统一在这里编码成
//! `LMERR:{json}` 信封（code + 插值参数），由前端 `src/i18n/error.ts` 的
//! `errorText()` 解码后按当前语言翻译。
//!
//! 约定：
//! - **code 是前后端契约**，改名等于破坏翻译，必须同步改两份语言包的 `error.*`；
//!   前端校验脚本（`.workbuddy/tmp/check-error-codes.cjs`）会反查两侧是否对齐；
//! - 第三方 / OS 错误（本就多为英文）**不编码**，保持 `e.to_string()` 原样透传；
//! - 语言包未命中某个 code 时前端回退显示 code 本身，不会露出中文。

/// 错误码集中定义：命名 `域.原因`，前端语言包按 `.` 分层挂在 `error.*` 下。
pub mod codes {
    // ---- 来源 ----
    /// 该来源正在扫描中（重复触发扫描）
    pub const SOURCE_SCANNING: &str = "source.scanning";
    /// 该来源正在扫描中，请稍后再移除
    pub const SOURCE_SCANNING_BUSY: &str = "source.scanningBusy";
    /// 来源不存在
    pub const SOURCE_NOT_FOUND: &str = "source.notFound";
    /// 目录不存在（添加本地来源）
    pub const SOURCE_DIR_MISSING: &str = "source.dirMissing";
    /// 该文件夹已在音乐库中
    pub const SOURCE_DUPLICATE: &str = "source.duplicate";
    /// 该地址已添加过（WebDAV）
    pub const SOURCE_DUPLICATE_URL: &str = "source.duplicateUrl";
    /// 未知来源类型：{kind}
    pub const SOURCE_KIND_UNKNOWN: &str = "source.kindUnknown";
    /// WebDAV 地址缺失（来源记录里没有 base_url）
    pub const SOURCE_URL_MISSING: &str = "source.urlMissing";

    // ---- 曲目 / 文件 ----
    /// 曲目不存在
    pub const TRACK_NOT_FOUND: &str = "track.notFound";
    /// 仅本地曲目支持此操作（WebDAV 曲目无法在文件管理器中显示）
    pub const TRACK_NOT_LOCAL: &str = "track.notLocal";
    /// 文件不存在
    pub const FILE_MISSING: &str = "file.missing";

    // ---- 歌单 ----
    /// 歌单名不能为空
    pub const PLAYLIST_NAME_EMPTY: &str = "playlist.nameEmpty";

    // ---- WebDAV / 网络 ----
    /// WebDAV 地址无效：{error}
    pub const WEBDAV_INVALID_URL: &str = "webdav.invalidUrl";
    /// PROPFIND 失败：{error}
    pub const WEBDAV_PROPFIND_FAILED: &str = "webdav.propfindFailed";
    /// PROPFIND 返回 {status}
    pub const WEBDAV_PROPFIND_STATUS: &str = "webdav.propfindStatus";
    /// 下载失败：{error}
    pub const DOWNLOAD_FAILED: &str = "download.failed";
    /// 下载返回 {status}
    pub const DOWNLOAD_STATUS: &str = "download.status";
    /// 下载歌词失败：{error}
    pub const LYRICS_DOWNLOAD_FAILED: &str = "lyrics.downloadFailed";
    /// 下载歌词返回 {status}
    pub const LYRICS_DOWNLOAD_STATUS: &str = "lyrics.downloadStatus";

    // ---- 系统钥匙串 ----
    /// 系统钥匙串不可用：{error}
    pub const KEYRING_UNAVAILABLE: &str = "keyring.unavailable";
    /// 凭证写入钥匙串失败：{error}
    pub const KEYRING_WRITE_FAILED: &str = "keyring.writeFailed";

    // ---- 电源 ----
    /// SetThreadExecutionState 调用失败（阻止休眠）
    pub const PREVENT_SLEEP_FAILED: &str = "power.preventSleepFailed";

    // ---- 转码（macOS Ogg → WAV，见 transcode.rs；transcode.rs 仅 macOS 编译，
    //      这些码在 Windows 下是死代码，需随平台门控）----
    /// 无法定位缓存目录
    #[cfg(target_os = "macos")]
    pub const TRANSCODE_CACHE_DIR: &str = "transcode.cacheDirMissing";
    /// 音频探测失败：{error}
    #[cfg(target_os = "macos")]
    pub const TRANSCODE_PROBE_FAILED: &str = "transcode.probeFailed";
    /// 文件中没有音频轨
    #[cfg(target_os = "macos")]
    pub const TRANSCODE_NO_AUDIO_TRACK: &str = "transcode.noAudioTrack";
    /// 解码器初始化失败：{error}
    #[cfg(target_os = "macos")]
    pub const TRANSCODE_DECODER_INIT: &str = "transcode.decoderInitFailed";
    /// 读取音频帧失败：{error}
    #[cfg(target_os = "macos")]
    pub const TRANSCODE_READ_FRAME: &str = "transcode.readFrameFailed";
    /// 解码失败：{error}
    #[cfg(target_os = "macos")]
    pub const TRANSCODE_DECODE_FAILED: &str = "transcode.decodeFailed";
    /// 文件中没有可解码的音频数据
    #[cfg(target_os = "macos")]
    pub const TRANSCODE_NO_DECODABLE: &str = "transcode.noDecodableAudio";
}

/// 信封前缀：前端据此识别「可翻译错误」。其余字符串一律原样显示。
pub const PREFIX: &str = "LMERR:";

struct AppErr {
    code: &'static str,
    params: Vec<(&'static str, String)>,
}

impl AppErr {
    fn new(code: &'static str) -> Self {
        Self { code, params: Vec::new() }
    }

    fn param(mut self, key: &'static str, value: impl std::fmt::Display) -> Self {
        self.params.push((key, value.to_string()));
        self
    }

    fn build(self) -> String {
        if self.params.is_empty() {
            return format!("{}{}", PREFIX, serde_json::json!({ "code": self.code }));
        }
        let mut map = serde_json::Map::new();
        for (k, v) in self.params {
            map.insert(k.to_string(), serde_json::Value::String(v));
        }
        format!("{}{}", PREFIX, serde_json::json!({ "code": self.code, "params": map }))
    }
}

/// 无参数错误：`Err(error::err(codes::SOURCE_SCANNING))`
pub fn err(code: &'static str) -> String {
    AppErr::new(code).build()
}

/// 单参数错误：`Err(error::err1(codes::WEBDAV_INVALID_URL, "error", e))`
pub fn err1(code: &'static str, key: &'static str, value: impl std::fmt::Display) -> String {
    AppErr::new(code).param(key, value).build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_without_params() {
        assert_eq!(err(codes::SOURCE_SCANNING), r#"LMERR:{"code":"source.scanning"}"#);
    }

    #[test]
    fn envelope_with_param() {
        let s = err1(codes::WEBDAV_INVALID_URL, "error", "relative URL without a base");
        assert!(s.starts_with(PREFIX));
        let parsed: serde_json::Value =
            serde_json::from_str(&s[PREFIX.len()..]).expect("信封必须是合法 JSON");
        assert_eq!(parsed["code"], "webdav.invalidUrl");
        assert_eq!(parsed["params"]["error"], "relative URL without a base");
    }

    #[test]
    fn param_value_with_quotes_and_newlines_is_json_safe() {
        let s = err1(codes::DOWNLOAD_FAILED, "error", "he said \"hi\"\nbye");
        assert!(s.starts_with(PREFIX));
        let parsed: serde_json::Value =
            serde_json::from_str(&s[PREFIX.len()..]).expect("信封必须是合法 JSON");
        assert_eq!(parsed["params"]["error"], "he said \"hi\"\nbye");
    }
}
