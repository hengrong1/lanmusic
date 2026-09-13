//! 用真实加密 QRC 文件验证新版解密 + 词级解析（样本 target/alin.qrc 缺失时跳过）
#[test]
fn decrypt_real_qrc_sample() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/target/alin.qrc");
    let Ok(bytes) = std::fs::read(path) else {
        eprintln!("skip: 无法读取样本 {path}");
        return;
    };
    // lyrics::decode_lyric_bytes 会把加密二进制归一成十六进制串，这里模拟同一路径
    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    let lines = lanmusic_lib::qrc::parse(&hex).expect("真实加密样本应解密并解析出行");
    assert!(lines.iter().any(|l| !l.text.is_empty()), "至少一行有文本");
    let word_lines = lines.iter().filter(|l| l.words.len() > 1).count();
    println!("lines={} word_lines={}", lines.len(), word_lines);
    for l in lines.iter().take(3) {
        println!("{l:?}");
    }
}
