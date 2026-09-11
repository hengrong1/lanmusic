//! macOS 专用：系统 WebView（WKWebView/Safari 媒体栈）不支持 Ogg(Vorbis/Opus) 解码。
//! 播放这类格式时，在 Rust 侧用 symphonia 解码为 WAV（PCM s16le）并缓存到应用缓存目录，
//! 之后按普通文件 + Range 供流——seek、进度条、缓冲全部复用 scheme.rs 的现有实现。
//!
//! - 仅 macOS 编译：Windows WebView2（Chromium）与 Linux WebKitGTK（GStreamer）可原生解码 Ogg；
//! - 缓存文件名包含源文件指纹（本地 = size-mtime，远程 = url 哈希-size），源文件变化自动失效；
//! - 缓存只存活一个会话，应用启动时整体清空（见 lib.rs setup 的 cleanup_cache）。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use tauri::{AppHandle, Manager, Runtime};

/// WebView 不支持解码、需要转码的格式（macOS）
pub fn needs_transcode(format: Option<&str>) -> bool {
    matches!(format, Some("ogg") | Some("oga") | Some("opus"))
}

/// 转码缓存目录：{app_cache_dir}/transcode
fn cache_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_cache_dir().ok().map(|d| d.join("transcode"))
}

/// 启动时清空上次会话的转码缓存（WAV 体积大且可随时重建，跨会话不保留）
pub fn cleanup_cache<R: Runtime>(app: &AppHandle<R>) {
    if let Some(dir) = cache_dir(app) {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// 确保转码缓存存在并返回 WAV 文件路径，然后按文件 + Range 供流。
/// 失败时降级为原始字节流（与修复前行为一致）。
pub fn serve_transcoded<R: Runtime>(
    app: &AppHandle<R>,
    track_id: i64,
    fingerprint: &str,
    source_bytes: &[u8],
    range: Option<&str>,
) -> tauri::http::Response<Vec<u8>> {
    let wav = match ensure_wav(app, track_id, fingerprint, source_bytes) {
        Ok(p) => p,
        Err(_) => return crate::scheme::serve_raw(source_bytes, range),
    };
    let Ok(file) = File::open(&wav) else {
        return crate::scheme::serve_raw(source_bytes, range);
    };
    let size = wav.metadata().map(|m| m.len()).unwrap_or(0);
    crate::scheme::serve_file_response(file, "audio/wav", size, range)
}

/// 缓存命中检查 + 未命中时解码写入
fn ensure_wav<R: Runtime>(
    app: &AppHandle<R>,
    track_id: i64,
    fingerprint: &str,
    source_bytes: &[u8],
) -> Result<PathBuf, String> {
    let dir =
        cache_dir(app).ok_or_else(|| crate::error::err(crate::error::codes::TRANSCODE_CACHE_DIR))?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out = dir.join(format!("{track_id}-{fingerprint}.wav"));
    if out.metadata().map(|m| m.len() > 44).unwrap_or(false) {
        return Ok(out);
    }
    decode_to_wav(source_bytes, &out)?;
    Ok(out)
}

/// symphonia 要求的内存源：Cursor 不直接实现 MediaSource，包一层
struct BytesSource(std::io::Cursor<Vec<u8>>);

impl Read for BytesSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl Seek for BytesSource {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.0.seek(pos)
    }
}

impl MediaSource for BytesSource {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.0.get_ref().len() as u64)
    }
}

/// 解码整首歌为 WAV(PCM s16le) 写入 out。
/// 先写 44 字节占位头，解码完按真实采样率/声道/数据长度回填。
fn decode_to_wav(source_bytes: &[u8], out: &Path) -> Result<(), String> {
    let mss = MediaSourceStream::new(
        Box::new(BytesSource(std::io::Cursor::new(source_bytes.to_vec()))),
        Default::default(),
    );
    let mut hint = Hint::new();
    // 容器探测以内容为准，扩展名仅作提示（ogg/oga/opus 都走 Ogg 容器）
    hint.with_extension("ogg");
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| crate::error::err1(crate::error::codes::TRANSCODE_PROBE_FAILED, "error", e))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| crate::error::err(crate::error::codes::TRANSCODE_NO_AUDIO_TRACK))?
        .clone();
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| crate::error::err1(crate::error::codes::TRANSCODE_DECODER_INIT, "error", e))?;

    let mut out_file = File::create(out).map_err(|e| e.to_string())?;
    out_file.write_all(&[0u8; 44]).map_err(|e| e.to_string())?;
    let mut pcm_len: u64 = 0;
    let mut spec = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            // 流正常结束
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(crate::error::err1(crate::error::codes::TRANSCODE_READ_FRAME, "error", e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let s = *decoded.spec();
                if spec.is_none() {
                    spec = Some(s);
                }
                let mut sbuf = SampleBuffer::<i16>::new(decoded.capacity() as u64, s);
                sbuf.copy_interleaved_ref(decoded);
                // s16 采样转为小端字节流
                let mut data = Vec::with_capacity(sbuf.samples().len() * 2);
                for sample in sbuf.samples() {
                    data.extend_from_slice(&sample.to_le_bytes());
                }
                out_file.write_all(&data).map_err(|e| e.to_string())?;
                pcm_len += data.len() as u64;
            }
            // 单帧损坏：跳过继续，尽量转出可用音频
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(crate::error::err1(crate::error::codes::TRANSCODE_DECODE_FAILED, "error", e)),
        }
    }

    let s = spec
        .ok_or_else(|| crate::error::err(crate::error::codes::TRANSCODE_NO_DECODABLE))?;
    let channels = s.channels.count() as u16;
    out_file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    out_file
        .write_all(&wav_header(s.rate, channels, pcm_len))
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 标准 WAV(PCM16) 文件头（44 字节）
fn wav_header(sample_rate: u32, channels: u16, data_len: u64) -> Vec<u8> {
    let byte_rate = sample_rate * channels as u32 * 2;
    let block_align = channels * 2;
    let mut h = Vec::with_capacity(44);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    h.extend_from_slice(b"WAVE");
    h.extend_from_slice(b"fmt ");
    h.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    h.extend_from_slice(&1u16.to_le_bytes()); // PCM
    h.extend_from_slice(&channels.to_le_bytes());
    h.extend_from_slice(&sample_rate.to_le_bytes());
    h.extend_from_slice(&byte_rate.to_le_bytes());
    h.extend_from_slice(&block_align.to_le_bytes());
    h.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    h.extend_from_slice(b"data");
    h.extend_from_slice(&(data_len as u32).to_le_bytes());
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_unsupported_formats() {
        assert!(needs_transcode(Some("ogg")));
        assert!(needs_transcode(Some("oga")));
        assert!(needs_transcode(Some("opus")));
        assert!(!needs_transcode(Some("mp3")));
        assert!(!needs_transcode(Some("flac")));
        assert!(!needs_transcode(None));
    }

    #[test]
    fn wav_header_layout() {
        let h = wav_header(44_100, 2, 1000);
        assert_eq!(h.len(), 44);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(&h[8..12], b"WAVE");
        assert_eq!(&h[12..16], b"fmt ");
        // PCM、双声道、44100Hz、16bit
        assert_eq!(u16::from_le_bytes([h[20], h[21]]), 1);
        assert_eq!(u16::from_le_bytes([h[22], h[23]]), 2);
        assert_eq!(u32::from_le_bytes([h[24], h[25], h[26], h[27]]), 44_100);
        assert_eq!(u32::from_le_bytes([h[28], h[29], h[30], h[31]]), 44_100 * 2 * 2);
        assert_eq!(u16::from_le_bytes([h[34], h[35]]), 16);
        assert_eq!(&h[36..40], b"data");
        assert_eq!(u32::from_le_bytes([h[40], h[41], h[42], h[43]]), 1000);
        // RIFF size = 36 + data
        assert_eq!(u32::from_le_bytes([h[4], h[5], h[6], h[7]]), 1036);
    }

    /// 真实文件验证（默认忽略）：
    /// LANMUSIC_OGG_FIXTURE=/path/to/file.ogg cargo test decode_real_ogg_fixture -- --ignored
    #[test]
    #[ignore]
    fn decode_real_ogg_fixture() {
        let path = std::env::var("LANMUSIC_OGG_FIXTURE").expect("set LANMUSIC_OGG_FIXTURE");
        let bytes = std::fs::read(&path).expect("read fixture");
        let out = std::env::temp_dir().join("lanmusic_transcode_test.wav");
        decode_to_wav(&bytes, &out).expect("decode to wav");
        let meta = std::fs::metadata(&out).expect("wav exists");
        assert!(meta.len() > 44, "wav 应有数据");
        let mut f = std::fs::File::open(&out).expect("open wav");
        let mut header = [0u8; 44];
        std::io::Read::read_exact(&mut f, &mut header).expect("read header");
        assert_eq!(&header[0..4], b"RIFF");
        assert_eq!(&header[8..12], b"WAVE");
        let rate = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
        assert!((8000..=192_000).contains(&rate), "采样率异常: {rate}");
        let _ = std::fs::remove_file(&out);
    }
}

