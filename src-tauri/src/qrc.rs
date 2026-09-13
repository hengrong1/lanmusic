//! QRC 逐字歌词（QQ音乐 .qrc）解密与解析，纯 Rust 实现（词级毫秒时间轴，供前端逐字高亮）。
//!
//! 解密流程参考 LyricToolsWeb（github.com/blockshy/LyricToolsWeb，services/qrc.ts）：
//! - 新版加密（桌面端较新的 .qrc）：11 字节固定头 + 循环 XOR + 三重 DES（D→E→D）+ zlib；
//! - 旧版加密（AMLL eqrc 同款）：三重 DES（D→E→D，密钥 "!@#)(*$%123ZXC!@!@#)(NHL"）+ zlib。
//!
//! 两种格式的三重 DES 链路一致（AMLL 24 字节密钥的三段 = LyricToolsWeb 三个密钥串的前 8 字节），
//! 区别仅在文件头与 XOR；注意 QQ 音乐用的是「改版 DES」（位序按 4 字节小端字重排），
//! 不是标准 DES，见下方 QQ 改版 DES 一节。
//!
//! 解析兼容两种载体与两种字标签排布：
//! - XML 包裹（解密产物）：逐字时间轴嵌在 `LyricContent` 属性值里（含未转义引号与 XML 实体）；
//! - 行式明文：`[行起,时长](字起,字时长)字…`，每行一条；
//! - 字标签一般在字**后**（`字(起,时长)`），也有的来源放在字**前**（`(起,时长)字`），按段首特征自动判别。

use serde::Serialize;

/// 单词/单字：毫秒时间轴
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrcWord {
    pub word: String,
    pub start_time: u64,
    pub end_time: u64,
}

/// 一行歌词：words 为空表示间奏占位；text 为全部单词拼接（已去首尾空白）
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrcLine {
    pub start_time: u64,
    pub end_time: u64,
    pub text: String,
    pub words: Vec<QrcWord>,
}

/// 解析 QRC 歌词；非 QRC 内容或解析不出有效行时返回 None（调用方回退 LRC/纯文本）。
/// 兼容明文 QRC 与加密 QRC（加密文件无论以十六进制文本还是原始字节形式外挂，
/// `lyrics::decode_lyric_bytes` 都会归一成十六进制串进入这里）。
/// 行标签 `[起,时长]` 是 QRC 特有格式（LRC 是 `[分:秒.厘]`），不会误伤 LRC 解析。
pub fn parse(raw: &str) -> Option<Vec<QrcLine>> {
    let body = if looks_like_hex(raw) {
        decrypt_bytes(&decode_hex(raw))?
    } else {
        raw.to_owned()
    };
    // XML 包裹：抽出全部 LyricContent 属性值（内容里的未转义引号一并删除，参考实现同款）；
    // 抽取失败（属性残缺）就整段按流解析，行标签扫描不受 XML 结构影响
    let (content, xml_mode) = match extract_lyric_contents(&body) {
        Some(c) => (c, true),
        None if body.contains("LyricContent") => (body, true),
        None => (body, false),
    };
    let mut lines = parse_stream(&content, xml_mode);
    lines.sort_by_key(|l| l.start_time);
    // 折叠连续间奏占位行：多个连续空行只保留第一个（与 parseLrc 同口径，
    // 且保证行下标与前端 lyricsLines/lyricsWordLines 一一对应）
    let mut collapsed: Vec<QrcLine> = Vec::with_capacity(lines.len());
    for line in lines {
        if line.text.is_empty() && collapsed.last().is_some_and(|p| p.text.is_empty()) {
            continue;
        }
        collapsed.push(line);
    }
    // 健全性检查：至少一行带可显示文本
    if collapsed.iter().all(|l| l.text.is_empty()) {
        return None;
    }
    Some(collapsed)
}

// ================================================================ 识别

/// 是否为十六进制串：加密 QRC 的归一载体（≥200 字符，纯 hex 与空白）
fn looks_like_hex(raw: &str) -> bool {
    let t = raw.trim();
    t.len() >= 200
        && t.bytes()
            .all(|b| b.is_ascii_hexdigit() || b.is_ascii_whitespace())
}

// ================================================================ QQ 改版 DES

// QQ 音乐歌词加密用的「DES」并非标准 DES：位读取按 4 字节小端字重排
// （bitnum 的 byte_index = pos/32*4 + 3 - pos%32/8），只能用于 QQ 音乐歌词加解密。
// 移植自 LyricToolsWeb（services/qrc.ts，源自 Java 版 QQMusicDES），
// 与 AMLL eqrc 的 qdec 同源同款。

fn bitnum(data: &[u8], pos: usize, shift: u32) -> u32 {
    let byte_index = pos / 32 * 4 + 3 - (pos % 32) / 8;
    (((data[byte_index] >> (7 - pos % 8)) & 1) as u32) << shift
}

fn bitnum_intr(v: u32, pos: usize, shift: u32) -> u32 {
    ((v >> (31 - pos)) & 1) << shift
}

fn bitnum_intl(v: u32, b: u32, shift: u32) -> u32 {
    ((v << b) & 0x8000_0000) >> shift
}

fn sbox_index(b: u8) -> usize {
    ((b & 0x20) | ((b & 0x1f) >> 1) | ((b & 0x01) << 4)) as usize
}

/// 标准 DES S 盒 S1..S8
const SBOXES: [[u8; 64]; 8] = [
    [
        14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7, 0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12,
        11, 9, 5, 3, 8, 4, 1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0, 15, 12, 8, 2, 4, 9,
        1, 7, 5, 11, 3, 14, 10, 0, 6, 13,
    ],
    [
        15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10, 3, 13, 4, 7, 15, 2, 8, 15, 12, 0, 1,
        10, 6, 9, 11, 5, 0, 14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15, 13, 8, 10, 1, 3, 15,
        4, 2, 11, 6, 7, 12, 0, 5, 14, 9,
    ],
    [
        10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8, 13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5,
        14, 12, 11, 15, 1, 13, 6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7, 1, 10, 13, 0, 6,
        9, 8, 7, 4, 15, 14, 3, 11, 5, 2, 12,
    ],
    [
        7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15, 13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2,
        12, 1, 10, 14, 9, 10, 6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4, 3, 15, 0, 6, 10,
        10, 13, 8, 9, 4, 5, 11, 12, 7, 2, 14,
    ],
    [
        2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9, 14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15,
        10, 3, 9, 8, 6, 4, 2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14, 11, 8, 12, 7, 1, 14,
        2, 13, 6, 15, 0, 9, 10, 4, 5, 3,
    ],
    [
        12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11, 10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13,
        14, 0, 11, 3, 8, 9, 14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6, 4, 3, 2, 12, 9, 5,
        15, 10, 11, 14, 1, 7, 6, 0, 8, 13,
    ],
    [
        4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1, 13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5,
        12, 2, 15, 8, 6, 1, 4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2, 6, 11, 13, 8, 1, 4,
        10, 7, 9, 5, 0, 15, 14, 2, 3, 12,
    ],
    [
        13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7, 1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6,
        11, 0, 14, 9, 2, 7, 11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8, 2, 1, 14, 7, 4, 10,
        8, 13, 15, 12, 9, 0, 3, 5, 6, 11,
    ],
];

/// 初始置换 IP
fn ip(input: &[u8; 8]) -> [u32; 2] {
    let state0 = bitnum(input, 57, 31)
        | bitnum(input, 49, 30)
        | bitnum(input, 41, 29)
        | bitnum(input, 33, 28)
        | bitnum(input, 25, 27)
        | bitnum(input, 17, 26)
        | bitnum(input, 9, 25)
        | bitnum(input, 1, 24)
        | bitnum(input, 59, 23)
        | bitnum(input, 51, 22)
        | bitnum(input, 43, 21)
        | bitnum(input, 35, 20)
        | bitnum(input, 27, 19)
        | bitnum(input, 19, 18)
        | bitnum(input, 11, 17)
        | bitnum(input, 3, 16)
        | bitnum(input, 61, 15)
        | bitnum(input, 53, 14)
        | bitnum(input, 45, 13)
        | bitnum(input, 37, 12)
        | bitnum(input, 29, 11)
        | bitnum(input, 21, 10)
        | bitnum(input, 13, 9)
        | bitnum(input, 5, 8)
        | bitnum(input, 63, 7)
        | bitnum(input, 55, 6)
        | bitnum(input, 47, 5)
        | bitnum(input, 39, 4)
        | bitnum(input, 31, 3)
        | bitnum(input, 23, 2)
        | bitnum(input, 15, 1)
        | bitnum(input, 7, 0);

    let state1 = bitnum(input, 56, 31)
        | bitnum(input, 48, 30)
        | bitnum(input, 40, 29)
        | bitnum(input, 32, 28)
        | bitnum(input, 24, 27)
        | bitnum(input, 16, 26)
        | bitnum(input, 8, 25)
        | bitnum(input, 0, 24)
        | bitnum(input, 58, 23)
        | bitnum(input, 50, 22)
        | bitnum(input, 42, 21)
        | bitnum(input, 34, 20)
        | bitnum(input, 26, 19)
        | bitnum(input, 18, 18)
        | bitnum(input, 10, 17)
        | bitnum(input, 2, 16)
        | bitnum(input, 60, 15)
        | bitnum(input, 52, 14)
        | bitnum(input, 44, 13)
        | bitnum(input, 36, 12)
        | bitnum(input, 28, 11)
        | bitnum(input, 20, 10)
        | bitnum(input, 12, 9)
        | bitnum(input, 4, 8)
        | bitnum(input, 62, 7)
        | bitnum(input, 54, 6)
        | bitnum(input, 46, 5)
        | bitnum(input, 38, 4)
        | bitnum(input, 30, 3)
        | bitnum(input, 22, 2)
        | bitnum(input, 14, 1)
        | bitnum(input, 6, 0);

    [state0, state1]
}

/// 逆初始置换 IP⁻¹
fn inv_ip(state: [u32; 2], output: &mut [u8; 8]) {
    output[3] = (bitnum_intr(state[1], 7, 7)
        | bitnum_intr(state[0], 7, 6)
        | bitnum_intr(state[1], 15, 5)
        | bitnum_intr(state[0], 15, 4)
        | bitnum_intr(state[1], 23, 3)
        | bitnum_intr(state[0], 23, 2)
        | bitnum_intr(state[1], 31, 1)
        | bitnum_intr(state[0], 31, 0)) as u8;

    output[2] = (bitnum_intr(state[1], 6, 7)
        | bitnum_intr(state[0], 6, 6)
        | bitnum_intr(state[1], 14, 5)
        | bitnum_intr(state[0], 14, 4)
        | bitnum_intr(state[1], 22, 3)
        | bitnum_intr(state[0], 22, 2)
        | bitnum_intr(state[1], 30, 1)
        | bitnum_intr(state[0], 30, 0)) as u8;

    output[1] = (bitnum_intr(state[1], 5, 7)
        | bitnum_intr(state[0], 5, 6)
        | bitnum_intr(state[1], 13, 5)
        | bitnum_intr(state[0], 13, 4)
        | bitnum_intr(state[1], 21, 3)
        | bitnum_intr(state[0], 21, 2)
        | bitnum_intr(state[1], 29, 1)
        | bitnum_intr(state[0], 29, 0)) as u8;

    output[0] = (bitnum_intr(state[1], 4, 7)
        | bitnum_intr(state[0], 4, 6)
        | bitnum_intr(state[1], 12, 5)
        | bitnum_intr(state[0], 12, 4)
        | bitnum_intr(state[1], 20, 3)
        | bitnum_intr(state[0], 20, 2)
        | bitnum_intr(state[1], 28, 1)
        | bitnum_intr(state[0], 28, 0)) as u8;

    output[7] = (bitnum_intr(state[1], 3, 7)
        | bitnum_intr(state[0], 3, 6)
        | bitnum_intr(state[1], 11, 5)
        | bitnum_intr(state[0], 11, 4)
        | bitnum_intr(state[1], 19, 3)
        | bitnum_intr(state[0], 19, 2)
        | bitnum_intr(state[1], 27, 1)
        | bitnum_intr(state[0], 27, 0)) as u8;

    output[6] = (bitnum_intr(state[1], 2, 7)
        | bitnum_intr(state[0], 2, 6)
        | bitnum_intr(state[1], 10, 5)
        | bitnum_intr(state[0], 10, 4)
        | bitnum_intr(state[1], 18, 3)
        | bitnum_intr(state[0], 18, 2)
        | bitnum_intr(state[1], 26, 1)
        | bitnum_intr(state[0], 26, 0)) as u8;

    output[5] = (bitnum_intr(state[1], 1, 7)
        | bitnum_intr(state[0], 1, 6)
        | bitnum_intr(state[1], 9, 5)
        | bitnum_intr(state[0], 9, 4)
        | bitnum_intr(state[1], 17, 3)
        | bitnum_intr(state[0], 17, 2)
        | bitnum_intr(state[1], 25, 1)
        | bitnum_intr(state[0], 25, 0)) as u8;

    output[4] = (bitnum_intr(state[1], 0, 7)
        | bitnum_intr(state[0], 0, 6)
        | bitnum_intr(state[1], 8, 5)
        | bitnum_intr(state[0], 8, 4)
        | bitnum_intr(state[1], 16, 3)
        | bitnum_intr(state[0], 16, 2)
        | bitnum_intr(state[1], 24, 1)
        | bitnum_intr(state[0], 24, 0)) as u8;
}

/// Feistel 轮函数：扩展置换 E → 与轮密钥异或 → 8 个 S 盒 → P 盒置换
fn feistel(state: u32, key: &[u8; 6]) -> u32 {
    let t1 = bitnum_intl(state, 31, 0)
        | ((state & 0xf000_0000) >> 1)
        | bitnum_intl(state, 4, 5)
        | bitnum_intl(state, 3, 6)
        | ((state & 0x0f00_0000) >> 3)
        | bitnum_intl(state, 8, 11)
        | bitnum_intl(state, 7, 12)
        | ((state & 0x00f0_0000) >> 5)
        | bitnum_intl(state, 12, 17)
        | bitnum_intl(state, 11, 18)
        | ((state & 0x000f_0000) >> 7)
        | bitnum_intl(state, 16, 23);

    let t2 = bitnum_intl(state, 15, 0)
        | ((state & 0x0000_f000) << 15)
        | bitnum_intl(state, 20, 5)
        | bitnum_intl(state, 19, 6)
        | ((state & 0x0000_0f00) << 13)
        | bitnum_intl(state, 24, 11)
        | bitnum_intl(state, 23, 12)
        | ((state & 0x0000_00f0) << 11)
        | bitnum_intl(state, 28, 17)
        | bitnum_intl(state, 27, 18)
        | ((state & 0x0000_000f) << 9)
        | bitnum_intl(state, 0, 23);

    let lrgstate = [
        ((t1 >> 24) & 0xff) as u8,
        ((t1 >> 16) & 0xff) as u8,
        ((t1 >> 8) & 0xff) as u8,
        ((t2 >> 24) & 0xff) as u8,
        ((t2 >> 16) & 0xff) as u8,
        ((t2 >> 8) & 0xff) as u8,
    ];

    let mut expanded = [0u8; 6];
    for (e, (s, &k)) in expanded.iter_mut().zip(lrgstate.iter().zip(key.iter())) {
        *e = s ^ k;
    }

    let result = ((SBOXES[0][sbox_index(expanded[0] >> 2)] as u32) << 28)
        | ((SBOXES[1][sbox_index(((expanded[0] & 0x03) << 4) | (expanded[1] >> 4))] as u32) << 24)
        | ((SBOXES[2][sbox_index(((expanded[1] & 0x0f) << 2) | (expanded[2] >> 6))] as u32) << 20)
        | ((SBOXES[3][sbox_index(expanded[2] & 0x3f)] as u32) << 16)
        | ((SBOXES[4][sbox_index(expanded[3] >> 2)] as u32) << 12)
        | ((SBOXES[5][sbox_index(((expanded[3] & 0x03) << 4) | (expanded[4] >> 4))] as u32) << 8)
        | ((SBOXES[6][sbox_index(((expanded[4] & 0x0f) << 2) | (expanded[5] >> 6))] as u32) << 4)
        | (SBOXES[7][sbox_index(expanded[5] & 0x3f)] as u32);

    bitnum_intl(result, 15, 0)
        | bitnum_intl(result, 6, 1)
        | bitnum_intl(result, 19, 2)
        | bitnum_intl(result, 20, 3)
        | bitnum_intl(result, 28, 4)
        | bitnum_intl(result, 11, 5)
        | bitnum_intl(result, 27, 6)
        | bitnum_intl(result, 16, 7)
        | bitnum_intl(result, 0, 8)
        | bitnum_intl(result, 14, 9)
        | bitnum_intl(result, 22, 10)
        | bitnum_intl(result, 25, 11)
        | bitnum_intl(result, 4, 12)
        | bitnum_intl(result, 17, 13)
        | bitnum_intl(result, 30, 14)
        | bitnum_intl(result, 9, 15)
        | bitnum_intl(result, 1, 16)
        | bitnum_intl(result, 7, 17)
        | bitnum_intl(result, 23, 18)
        | bitnum_intl(result, 13, 19)
        | bitnum_intl(result, 31, 20)
        | bitnum_intl(result, 26, 21)
        | bitnum_intl(result, 2, 22)
        | bitnum_intl(result, 8, 23)
        | bitnum_intl(result, 18, 24)
        | bitnum_intl(result, 12, 25)
        | bitnum_intl(result, 29, 26)
        | bitnum_intl(result, 5, 27)
        | bitnum_intl(result, 21, 28)
        | bitnum_intl(result, 10, 29)
        | bitnum_intl(result, 3, 30)
        | bitnum_intl(result, 24, 31)
}

/// 单 DES 轮密钥生成（16 组 6 字节）；decrypt 时子密钥顺序反转
fn des_key_setup(key: &[u8; 8], decrypt: bool) -> [[u8; 6]; 16] {
    const KEY_RND_SHIFT: [u32; 16] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];
    const KEY_PERM_C: [usize; 28] = [
        56, 48, 40, 32, 24, 16, 8, 0, 57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2,
        59, 51, 43, 35,
    ];
    const KEY_PERM_D: [usize; 28] = [
        62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29, 21, 13, 5, 60, 52, 44, 36, 28, 20, 12,
        4, 27, 19, 11, 3,
    ];
    const KEY_COMPRESSION: [usize; 48] = [
        13, 16, 10, 23, 0, 4, 2, 27, 14, 5, 20, 9, 22, 18, 11, 3, 25, 7, 15, 6, 26, 19, 12, 1, 40,
        51, 30, 36, 46, 54, 29, 39, 50, 44, 32, 47, 43, 48, 38, 55, 33, 52, 45, 41, 49, 35, 28, 31,
    ];

    let mut c: u32 = KEY_PERM_C
        .iter()
        .enumerate()
        .fold(0, |acc, (i, &p)| acc | bitnum(key, p, 31 - i as u32));
    let mut d: u32 = KEY_PERM_D
        .iter()
        .enumerate()
        .fold(0, |acc, (i, &p)| acc | bitnum(key, p, 31 - i as u32));

    let mut schedule = [[0u8; 6]; 16];
    for (i, &shift) in KEY_RND_SHIFT.iter().enumerate() {
        c = ((c << shift) | (c >> (28 - shift))) & 0xffff_fff0;
        d = ((d << shift) | (d >> (28 - shift))) & 0xffff_fff0;

        let to_gen = if decrypt { 15 - i } else { i };

        for (j, &comp) in KEY_COMPRESSION.iter().enumerate() {
            let (v, pos) = if j < 24 { (c, comp) } else { (d, comp - 27) };
            schedule[to_gen][j / 8] |= bitnum_intr(v, pos, 7 - (j % 8) as u32) as u8;
        }
    }
    schedule
}

/// 单 DES 逐块加/解密
fn des_crypt_block(input: &[u8; 8], schedule: &[[u8; 6]; 16]) -> [u8; 8] {
    let mut state = ip(input);

    for key in &schedule[..15] {
        let temp = state[1];
        state[1] = feistel(state[1], key) ^ state[0];
        state[0] = temp;
    }
    state[0] ^= feistel(state[1], &schedule[15]);

    let mut output = [0u8; 8];
    inv_ip(state, &mut output);
    output
}

// ================================================================ 解密

/// 新版加密固定文件头（QQ 音乐桌面端 .qrc）
const NEW_HEADER: [u8; 11] = [
    0x98, 0x25, 0xB0, 0xAC, 0xE3, 0x02, 0x83, 0x68, 0xE8, 0xFC, 0x6C,
];

/// 新版加密循环 XOR 密钥（94 字节，LyricToolsWeb 同款）
const XOR_KEY_HEX: &str = "629F5B0900C35E95239F13117ED8923FBC90BB740EC347743D90AA3F51D8F411849FDE951DC3C609D59FFA66F9D8F0F7A090A1D6F3C3F3D6A190A0F7F0D8F966FA9FD509C6C31D95DE9F8411F4D8513FAA903D7447C30E74BB90BC3F92D87E11139F23955EC300095B9F6266A1D852F76790CAD64AC34AD6CA9067F752D8A166";

/// 三重 DES 三个单轮密钥：AMLL eqrc 的 24 字节密钥 "!@#)(*$%123ZXC!@!@#)(NHL" 三段
/// 与 LyricToolsWeb KEY_1/2/3_STR 各自前 8 字节一致，两种格式的链路顺序相同
const QQ_KEY1: &[u8; 8] = b"!@#)(NHL";
const QQ_KEY2: &[u8; 8] = b"123ZXC!@";
const QQ_KEY3: &[u8; 8] = b"!@#)(*$%";

fn decrypt_bytes(data: &[u8]) -> Option<String> {
    decrypt_new_format(data).or_else(|| decrypt_legacy(data))
}

/// 新版加密：11 字节头（无头也能处理）→ 循环 XOR → 3DES D→E→D → zlib
fn decrypt_new_format(data: &[u8]) -> Option<String> {
    let body: &[u8] = if data.len() >= NEW_HEADER.len() && data[..NEW_HEADER.len()] == NEW_HEADER {
        &data[NEW_HEADER.len()..]
    } else {
        data
    };
    let xor_key = decode_hex(XOR_KEY_HEX);
    let mut buf: Vec<u8> = body
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ xor_key[i % xor_key.len()])
        .collect();
    triple_des_decrypt(&mut buf);
    inflate(&buf)
}

/// 旧版加密（AMLL eqrc / Lyricify 同款）：3DES D→E→D → zlib（无 XOR、无文件头）
fn decrypt_legacy(data: &[u8]) -> Option<String> {
    let mut buf = data.to_vec();
    triple_des_decrypt(&mut buf);
    inflate(&buf)
}

/// 三重 DES 逐块解密（ECB，无填充）：D(QQ_KEY1) → E(QQ_KEY2) → D(QQ_KEY3)
/// （与 AMLL eqrc TripleQDES 解密调度 [D(k3) E(k2) D(k1)] 逐块一致）
fn triple_des_decrypt(data: &mut [u8]) {
    let s1 = des_key_setup(QQ_KEY1, true);
    let s2 = des_key_setup(QQ_KEY2, false);
    let s3 = des_key_setup(QQ_KEY3, true);
    for chunk in data.as_chunks_mut::<8>().0 {
        let mut block = [0u8; 8];
        block.copy_from_slice(chunk);
        block = des_crypt_block(&block, &s1);
        block = des_crypt_block(&block, &s2);
        block = des_crypt_block(&block, &s3);
        chunk.copy_from_slice(&block);
    }
}

/// zlib 解压为 UTF-8 文本；为空视为失败。
/// 截断的 QRC XML 常缺结尾标签（加密导出不完整），补全以便抽取 LyricContent（参考实现同款）。
fn inflate(data: &[u8]) -> Option<String> {
    let out = miniz_oxide::inflate::decompress_to_vec_zlib(data).ok()?;
    let mut text = String::from_utf8_lossy(&out).into_owned();
    if text.contains("<QrcInfos>") && !text.contains("</QrcInfos>") {
        text.push_str("\n\"/>\n</LyricInfo>\n</QrcInfos>");
    }
    (!text.trim().is_empty()).then_some(text)
}

fn decode_hex(s: &str) -> Vec<u8> {
    let digits: Vec<u8> = s.bytes().filter(|b| b.is_ascii_hexdigit()).collect();
    digits
        .chunks(2)
        .filter(|p| p.len() == 2)
        .map(|p| hex_val(p[0]) << 4 | hex_val(p[1]))
        .collect()
}

fn hex_val(b: u8) -> u8 {
    match b {
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => b - b'0',
    }
}

// ================================================================ 解析

/// 从 QRC XML 中抽出全部 LyricContent 属性值（多个歌词轨时按顺序拼接）。
/// 属性值终止于「后跟可选空白与 `/>` 的引号」，内容中的未转义 `"` 一并删除
/// （等价参考实现的 fixInvalidQrcXml + 属性提取）。
fn extract_lyric_contents(xml: &str) -> Option<String> {
    const MARK: &str = "LyricContent=\"";
    let mut out = String::new();
    let mut rest = xml;
    while let Some(pos) = rest.find(MARK) {
        let after = &rest[pos + MARK.len()..];
        let bytes = after.as_bytes();
        // 属性值终止于「后跟可选空白与 "/>" 的引号」
        let mut boundary = None;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'"' {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if bytes[j..].starts_with(b"/>") {
                    boundary = Some((i, j));
                    break;
                }
            }
            i += 1;
        }
        let Some((end, tail)) = boundary else { break };
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&after[..end].replace('"', ""));
        rest = &after[tail + 2..]; // 跳过引号后空白与 "/>"
    }
    (!out.is_empty()).then_some(out)
}

/// 流式解析：按 `[行起,时长]` 行标签切段，段内再按 `(起,时长[,x])` 字标签切词。
/// xml_mode：解码 XML 实体，并把段尾的 `/>` / `</LyricInfo>` 收尾截掉。
fn parse_stream(body: &str, xml_mode: bool) -> Vec<QrcLine> {
    let bytes = body.as_bytes();
    // 行标签扫描：[起,时长]
    struct LineTag {
        /// 标签结束（正文起点）
        content_start: usize,
        /// 标签起点（下一段的正文终点）
        tag_pos: usize,
        start: u64,
        dur: u64,
    }
    let mut line_tags: Vec<LineTag> = vec![];
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some((st, du, consumed)) = parse_line_tag(&bytes[i..]) {
                line_tags.push(LineTag {
                    content_start: i + consumed,
                    tag_pos: i,
                    start: st,
                    dur: du,
                });
                i += consumed;
                continue;
            }
        }
        i += 1;
    }
    if line_tags.is_empty() {
        return vec![];
    }

    let segments: Vec<(&str, u64, u64)> = line_tags
        .iter()
        .enumerate()
        .map(|(k, t)| {
            let end = line_tags.get(k + 1).map_or(bytes.len(), |n| n.tag_pos);
            (&body[t.content_start..end], t.start, t.dur)
        })
        .collect();

    // 字标签排布方向：首个含内容的段以合法字标签开头 → 标签在字前
    // （先验证标签合法性，避免把「歌词文本以 `(1984)` 开头」误判成字前排布）
    let prefix_tag = segments
        .iter()
        .find_map(|(seg, _, _)| {
            let t = seg.trim_start();
            if t.is_empty() {
                None
            } else {
                Some(t.starts_with('(') && parse_word_tag(t.as_bytes()).is_some())
            }
        })
        .unwrap_or(false);

    segments
        .into_iter()
        .map(|(seg, start, dur)| {
            let words = parse_words_in_segment(seg, start, start + dur, prefix_tag, xml_mode);
            let text = words.iter().map(|w| w.word.as_str()).collect::<String>();
            QrcLine {
                start_time: start,
                end_time: start + dur,
                text: text.trim().to_owned(),
                words,
            }
        })
        .collect()
}

/// 段内切词：每个字标签配对紧邻的一段文本，产出词级时间轴。
/// 没有标签可配对的边界文本（标签在字后时的行尾余文 / 标签在字前时的行首余文）
/// 并入相邻的词；整段无词标签时生成覆盖整行的单词，保证 words 拼接 == 行文本。
fn parse_words_in_segment(
    seg: &str,
    line_start: u64,
    line_end: u64,
    prefix_tag: bool,
    xml_mode: bool,
) -> Vec<QrcWord> {
    // XML 段：属性收尾（"/> 或 </LyricInfo>，可能带收尾引号）之后不属于歌词
    let seg = if xml_mode {
        truncate_xml_tail(seg)
    } else {
        seg
    };
    let bytes = seg.as_bytes();

    let mut tags: Vec<(usize, usize, u64, u64)> = vec![]; // (标签起点, 结束, 起, 时长)
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            if let Some((st, du, consumed)) = parse_word_tag(&bytes[i..]) {
                tags.push((i, i + consumed, st, du));
                i += consumed;
                continue;
            }
        }
        i += 1;
    }
    if tags.is_empty() {
        // 无字标签：整段文本作为一行词（间奏段为空则产出空 words）
        let text = clean_chunk(seg, xml_mode);
        return if text.is_empty() {
            vec![]
        } else {
            vec![QrcWord {
                word: text,
                start_time: line_start,
                end_time: line_end,
            }]
        };
    }

    let mut words = Vec::with_capacity(tags.len());
    for (k, &(pos, end, st, du)) in tags.iter().enumerate() {
        // 标签在字前：文本 = 本标签结束 .. 下一标签开始（末个标签到段尾）
        // 标签在字后：文本 = 上一标签结束 .. 本标签开始（首标签之前没有上一标签）
        let (text_start, text_end) = if prefix_tag {
            (end, tags.get(k + 1).map_or(bytes.len(), |n| n.0))
        } else {
            (if k > 0 { tags[k - 1].1 } else { 0 }, pos)
        };
        let text = clean_chunk(&seg[text_start..text_end], xml_mode);
        if text.is_empty() {
            continue;
        }
        words.push(QrcWord {
            word: text,
            start_time: st,
            end_time: st + du,
        });
    }

    // 边界余文并入相邻词，避免行尾/行首文本在逐字渲染（只画 words）时丢失
    let leftover = if prefix_tag {
        clean_chunk(&seg[..tags[0].0], xml_mode)
    } else {
        clean_chunk(&seg[tags[tags.len() - 1].1..], xml_mode)
    };
    if !leftover.is_empty() {
        if let Some(w) = if prefix_tag {
            words.first_mut()
        } else {
            words.last_mut()
        } {
            if prefix_tag {
                w.word.insert_str(0, &leftover);
            } else {
                w.word.push_str(&leftover);
            }
        }
    }
    words
}

/// 段内文本清洗：去掉换行（跨行拼接用），XML 段解码实体
fn clean_chunk(s: &str, xml_mode: bool) -> String {
    let t = s.trim_matches(['\r', '\n']);
    if xml_mode {
        decode_entities(t)
    } else {
        t.to_owned()
    }
}

/// 截掉 XML 属性收尾：最早的 `/>` 或 `</LyricInfo>`（连同紧邻的收尾引号）
fn truncate_xml_tail(seg: &str) -> &str {
    let cut = seg
        .find("/>")
        .into_iter()
        .chain(seg.find("</LyricInfo>"))
        .min();
    match cut {
        Some(p) => {
            let bytes = seg.as_bytes();
            let p = if p > 0 && bytes[p - 1] == b'"' {
                p - 1
            } else {
                p
            };
            &seg[..p]
        }
        None => seg,
    }
}

/// XML 实体解码；`&amp;` 必须最后处理，避免 `&amp;lt;` 之类二次解码
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    s.replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// `[起,时长]` → (起, 时长, 消耗字节数)
fn parse_line_tag(b: &[u8]) -> Option<(u64, u64, usize)> {
    if b.first() != Some(&b'[') {
        return None;
    }
    let mut i = 1;
    let st = parse_digits(b, &mut i)?;
    if b.get(i) != Some(&b',') {
        return None;
    }
    i += 1;
    let du = parse_digits(b, &mut i)?;
    if b.get(i) != Some(&b']') {
        return None;
    }
    Some((st, du, i + 1))
}

/// `(起,时长[,x])` → (起, 时长, 消耗字节数)；容忍第三段（部分 QRC 是 `(0,3420,0)` 排布）
fn parse_word_tag(b: &[u8]) -> Option<(u64, u64, usize)> {
    if b.first() != Some(&b'(') {
        return None;
    }
    let mut i = 1;
    let st = parse_digits(b, &mut i)?;
    if b.get(i) != Some(&b',') {
        return None;
    }
    i += 1;
    let du = parse_digits(b, &mut i)?;
    if b.get(i) == Some(&b',') {
        i += 1;
        while i < b.len() && b[i] != b')' {
            i += 1;
        }
    }
    if b.get(i) != Some(&b')') {
        return None;
    }
    Some((st, du, i + 1))
}

fn parse_digits(b: &[u8], i: &mut usize) -> Option<u64> {
    let s = *i;
    while *i < b.len() && b[*i].is_ascii_digit() {
        *i += 1;
    }
    if *i == s {
        return None;
    }
    std::str::from_utf8(&b[s..*i]).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 明文行式（标签在字后）：词级时间轴、间奏折叠
    #[test]
    fn plain_word_level() {
        let raw = "[0,3420]Counting(0,18) (18,18)Stars(36,180)\r\n\
                   [3420,2000]第二(3420,1000)行(4420,1000)\r\n\
                   [5420,1000]\r\n[6420,500]\r\n[6920,1000]间奏后(6920,500)";
        let lines = parse(raw).expect("应解析出行");
        assert_eq!(lines.len(), 4, "两个连续间奏行应折叠为一行占位");
        assert_eq!(lines[0].text, "Counting Stars");
        assert_eq!(lines[0].words.len(), 3);
        assert_eq!(lines[0].words[0].word, "Counting");
        assert_eq!(lines[0].words[0].end_time, 18);
        assert_eq!(lines[0].words[1].word, " ");
        assert_eq!(lines[0].words[1].start_time, 18);
        assert_eq!(lines[0].words[2].word, "Stars");
        assert_eq!(lines[0].words[2].end_time, 216);
        assert_eq!(lines[1].text, "第二行");
        assert_eq!(lines[1].start_time, 3420);
        assert_eq!(lines[2].text, "", "折叠后保留一个间奏占位行");
        assert_eq!(lines[3].text, "间奏后");
        assert!(lines.iter().take(2).all(|l| !l.words.is_empty()));
    }

    /// 标签在字前的排布：`(起,时长)字`
    #[test]
    fn prefix_tag_layout() {
        let raw = "[0,400](0,100)你(100,100)好(200,200)呀";
        let lines = parse(raw).expect("应解析出行");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "你好呀");
        assert_eq!(lines[0].words.len(), 3);
        assert_eq!(lines[0].words[0].word, "你");
        assert_eq!(lines[0].words[0].start_time, 0);
        assert_eq!(lines[0].words[1].word, "好");
        assert_eq!(lines[0].words[1].start_time, 100);
        assert_eq!(lines[0].words[2].word, "呀");
        assert_eq!(lines[0].words[2].start_time, 200);
    }

    /// XML 包裹：LyricContent 抽取、实体解码、内容含未转义引号
    #[test]
    fn xml_wrapped() {
        let xml = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                   <QrcInfos>\n<QrcHeadInfo Title=\"T\" Version=\"1001\"/>\n\
                   <LyricInfo LyricCount=\"1\">\n\
                   <Lyric_1 LyricType=\"1\" LyricContent=\"[0,300]他说\"你好\"(0,150)啊(150,150)\"/>\n\
                   </LyricInfo>\n</QrcInfos>";
        let lines = parse(xml).expect("应解析出行");
        assert_eq!(lines.len(), 1);
        // 未转义引号按参考实现删除；&amp; 解码为 &（&amp; 最后替换，不二次解码）
        assert_eq!(lines[0].text, "他说你好啊");
        assert_eq!(lines[0].words[0].word, "他说你好");
        assert_eq!(lines[0].words[0].end_time, 150);
        assert_eq!(lines[0].words[1].word, "啊");
        assert_eq!(lines[0].words[1].start_time, 150);
    }

    /// 非法 `(1984)` 文本开头不应误判为字前标签排布
    #[test]
    fn paren_text_at_line_start() {
        let raw = "[0,400](1984)(0,100)歌(100,100)词";
        let lines = parse(raw).expect("应解析出行");
        assert_eq!(lines[0].text, "(1984)歌词");
        assert_eq!(lines[0].words[0].word, "(1984)");
        assert_eq!(lines[0].words[0].start_time, 0);
        assert_eq!(lines[0].words[1].word, "歌词");
    }

    /// 旧版加密往返：zlib 压缩 → QQ 改版 3DES 加密 → 十六进制 → parse。
    /// 加密数据补尾随垃圾到 ≥200 字符（hex 阈值；zlib 流结束后多余字节会被忽略）
    #[test]
    fn legacy_roundtrip() {
        let plain = "[0,300]hello(0,150)world(150,150)";
        let mut z = miniz_oxide::deflate::compress_to_vec_zlib(plain.as_bytes(), 6);
        z.resize(120, 0x5A);
        let mut enc = z;
        triple_des_encrypt(&mut enc);
        let lines = parse(&to_hex(&enc)).expect("旧版加密应可解密解析");
        assert_eq!(lines[0].text, "helloworld");
        assert_eq!(lines[0].words[1].word, "world");
        assert_eq!(lines[0].words[1].start_time, 150);
    }

    /// 新版加密往返：zlib 压缩（补零对齐到 8 并垫到 ≥200 hex）→ 3DES 加密 → XOR → 11 字节头 → parse
    #[test]
    fn new_format_roundtrip() {
        let plain = "[0,300]第一(0,300)句\n[300,300]second(300,300) line";
        let mut z = miniz_oxide::deflate::compress_to_vec_zlib(plain.as_bytes(), 6);
        while z.len() % 8 != 0 {
            z.push(0);
        }
        z.resize(120, 0);
        let mut enc = z;
        triple_des_encrypt(&mut enc);
        let xor = decode_hex(XOR_KEY_HEX);
        for (i, b) in enc.iter_mut().enumerate() {
            *b ^= xor[i % xor.len()];
        }
        let mut file = NEW_HEADER.to_vec();
        file.extend(enc);
        let lines = parse(&to_hex(&file)).expect("新版加密应可解密解析");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "第一句");
        assert_eq!(lines[1].text, "second line");
    }

    /// 三重 DES 逐块加密（解密链 D(K1)→E(K2)→D(K3) 的逆，仅供测试构造加密样本）
    fn triple_des_encrypt(data: &mut [u8]) {
        let s1 = des_key_setup(QQ_KEY3, false); // E(QQ_KEY3)
        let s2 = des_key_setup(QQ_KEY2, true); // D(QQ_KEY2)
        let s3 = des_key_setup(QQ_KEY1, false); // E(QQ_KEY1)
        for chunk in data.as_chunks_mut::<8>().0 {
            let mut block = [0u8; 8];
            block.copy_from_slice(chunk);
            block = des_crypt_block(&block, &s1);
            block = des_crypt_block(&block, &s2);
            block = des_crypt_block(&block, &s3);
            chunk.copy_from_slice(&block);
        }
    }

    fn to_hex(data: &[u8]) -> String {
        data.iter().map(|b| format!("{b:02X}")).collect()
    }
}
