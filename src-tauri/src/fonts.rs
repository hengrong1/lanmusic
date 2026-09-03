//! 读取系统已安装字体（仅 Windows）：枚举注册表 Fonts 项的字体友好名称，
//! 供「设置 → 外观 → 字体」下拉选择。过滤掉粗体/斜体等变体项，只保留字族名。

use std::collections::BTreeSet;

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

/// 从单个注册表根（HKLM / HKCU）的 Fonts 项收集字族名
fn collect(root: winreg::RegKey, out: &mut BTreeSet<String>) {
    let Ok(key) = root.open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts") else {
        return;
    };
    for name in key.enum_values().filter_map(|r| r.ok()).map(|(n, _)| n) {
        // 去掉文件类型说明括号："Microsoft YaHei (TrueType)" → "Microsoft YaHei"
        let base = match name.split_once(" (") {
            Some((b, _)) => b,
            None => name.as_str(),
        };
        // "SimSun & NSimSun" 这类多族名只取第一个
        let family = base.split(" & ").next().unwrap_or(base).trim();
        if family.is_empty() {
            continue;
        }
        // 过滤字重/字形变体（注册表为每个变体单列一条），只留字族本身
        let lower = family.to_lowercase();
        const VARIANTS: [&str; 10] = [
            "bold", "italic", "oblique", "light", "thin", "black", "medium", "semibold",
            "extrabold", "regular",
        ];
        if VARIANTS.iter().any(|v| lower.ends_with(v)) {
            continue;
        }
        out.insert(family.to_string());
    }
}

/// 系统字体列表（去重、不区分大小写排序）；HKLM 系统级 + HKCU 用户级
pub fn system_fonts() -> Vec<String> {
    let mut set = BTreeSet::new();
    for predef in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        collect(RegKey::predef(predef), &mut set);
    }
    set.into_iter().collect()
}