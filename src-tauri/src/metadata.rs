use lofty::config::ParseOptions;
use lofty::picture::PictureType;
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::{ItemKey, ItemValue, Tag};
use std::io::Cursor;
use std::path::Path;

#[derive(Debug, Default)]
pub struct TrackMeta {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i64>,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub duration: Option<f64>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i64>,
    pub channels: Option<i64>,
    pub bit_depth: Option<i64>,
    pub has_lyrics: bool,
    /// 歌词正文（内嵌 USLT），用于歌词展示
    pub lyrics: Option<String>,
    pub cover: Option<Vec<u8>>,
    /// ReplayGain 轨道增益（dB，如 -6.12）；标签缺失为 None
    pub rg_track_gain: Option<f64>,
    /// ReplayGain 轨道峰值（线性 0..1+，如 0.987654）；标签缺失为 None
    pub rg_track_peak: Option<f64>,
}

/// 解析 "3/12" 这类带总轨数的字段
fn parse_num(s: &str) -> Option<i64> {
    s.split('/').next()?.trim().parse::<i64>().ok()
}

fn clean(s: Option<&str>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// 读取单个音频文件的元数据。解析失败时返回 Err，由调用方降级为文件名。
///
/// `include_cover=false`（扫描路径）时 lofty 跳过封面解析，避免把每首歌几百 KB～几 MB 的
/// 封面字节读进内存——封面改为展示时经 cover:// 协议惰性提取（见 covers.rs）。
pub fn read(path: &Path, include_cover: bool) -> Result<TrackMeta, String> {
    let tagged = Probe::open(path)
        .map_err(|e| e.to_string())?
        .options(ParseOptions::new().read_cover_art(include_cover))
        .guess_file_type()
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())?;
    Ok(from_tagged(tagged, include_cover))
}

/// 从内存字节解析（远程源：只拉取文件头部字节用于标签读取）
pub fn read_bytes(data: &[u8], include_cover: bool) -> Result<TrackMeta, String> {
    let cursor = Cursor::new(data);
    let tagged = Probe::new(cursor)
        .options(ParseOptions::new().read_cover_art(include_cover))
        .guess_file_type()
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())?;
    Ok(from_tagged(tagged, include_cover))
}

fn from_tagged(tagged: lofty::file::TaggedFile, include_cover: bool) -> TrackMeta {
    let props = tagged.properties();
    let tag: Option<&Tag> = tagged.primary_tag().or_else(|| tagged.first_tag());

    let mut meta = TrackMeta {
        duration: Some(props.duration().as_secs_f64()),
        bitrate: props.audio_bitrate().map(|v| v as i64),
        sample_rate: props.sample_rate().map(|v| v as i64),
        channels: props.channels().map(|v| v as i64),
        bit_depth: props.bit_depth().map(|v| v as i64),
        ..Default::default()
    };

    if let Some(tag) = tag {
        meta.title = clean(tag.title().as_deref());
        meta.artist = clean(tag.artist().as_deref());
        meta.album = clean(tag.album().as_deref());
        meta.album_artist = clean(tag.get_string(ItemKey::AlbumArtist));
        meta.genre = clean(tag.genre().as_deref());
        meta.year = tag.get_string(ItemKey::Year).and_then(parse_num);
        meta.track_no = tag.get_string(ItemKey::TrackNumber).and_then(parse_num);
        meta.disc_no = tag.get_string(ItemKey::DiscNumber).and_then(parse_num);
        // 注意：ID3v2 USLT（MP3 常见）映射到 UnsyncLyrics；Vorbis/M4A 才是 Lyrics，两者都要读
        let lyric_text = tag
            .get_string(ItemKey::Lyrics)
            .or_else(|| tag.get_string(ItemKey::UnsyncLyrics))
            .map(str::to_string);
        if let Some(text) = lyric_text {
            meta.has_lyrics = !text.trim().is_empty();
            if meta.has_lyrics {
                meta.lyrics = Some(text);
            }
        }
        let (rg_gain, rg_peak) = parse_replaygain(tag);
        meta.rg_track_gain = rg_gain;
        meta.rg_track_peak = rg_peak;
        if include_cover {
            meta.cover = pick_cover(tag);
        }
    }
    meta
}

/// 读取 ReplayGain 轨道增益/峰值。lofty 0.25 把这两项统一落为字符串
/// （增益形如 "-6.12 dB"，峰值为线性小数如 "0.987654"），因此提取其中的数字再解析。
fn parse_replaygain(tag: &Tag) -> (Option<f64>, Option<f64>) {
    let mut gain = None;
    let mut peak = None;
    for item in tag.items() {
        match item.key() {
            ItemKey::ReplayGainTrackGain => gain = rg_value(item.value()),
            ItemKey::ReplayGainTrackPeak => peak = rg_value(item.value()),
            _ => {}
        }
    }
    (gain, peak)
}

/// 从 ReplayGain 文本（如 "-6.12 dB" / "0.987654"）提取数值部分
fn rg_value(v: &ItemValue) -> Option<f64> {
    let s = match v {
        ItemValue::Text(s) | ItemValue::Locator(s) => s,
        ItemValue::Binary(_) => return None,
    };
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    cleaned.parse::<f64>().ok()
}

fn pick_cover(tag: &Tag) -> Option<Vec<u8>> {
    let pics = tag.pictures();
    let pic = pics
        .iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or_else(|| pics.first())?;
    let data = pic.data();
    if data.is_empty() {
        None
    } else {
        Some(data.to_vec())
    }
}

/// 读取（可能是部分的）远程文件头部字节的推荐大小：足以覆盖绝大多数 ID3v2/FLAC 元数据块
pub const HEAD_FETCH_SIZE: u64 = 1024 * 1024;
