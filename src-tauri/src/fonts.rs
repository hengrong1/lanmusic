//! 枚举系统已安装字体（仅 Windows）：通过 DirectWrite 遍历系统字体集合的
//! 字族友好名称，供「设置 → 外观 → 字体」下拉选择。相比解析注册表 Fonts 项，
//! DirectWrite 直接返回渲染引擎（WebView2/Chromium）实际解析的本地化字族名，
//! 系统级与用户级（per-user，右键「为当前用户安装」）字体均会包含。

use std::collections::BTreeSet;

use windows::core::{PCWSTR, BOOL};
use windows::Win32::Globalization::GetUserDefaultLocaleName;
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteLocalizedStrings, DWRITE_FACTORY_TYPE_SHARED,
};

/// 当前用户区域名（如 zh-CN），用于挑选字族的本地化名称；失败返回 None
fn user_locale() -> Option<Vec<u16>> {
    let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
    let len = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if len > 0 {
        Some(buf[..len as usize].to_vec()) // 含结尾 \0
    } else {
        None
    }
}

/// 从本地化名称列表取目标区域的名字，无匹配则回退到第一项
fn family_name(names: &IDWriteLocalizedStrings, locale: Option<&[u16]>) -> Option<String> {
    unsafe {
        let mut index = 0u32;
        let mut matched = BOOL::default();
        if let Some(loc) = locale {
            let _ = names.FindLocaleName(PCWSTR(loc.as_ptr()), &mut index, &mut matched);
        }
        if !matched.as_bool() {
            if names.GetCount() == 0 {
                return None;
            }
            index = 0;
        }
        let len = names.GetStringLength(index).ok()? as usize;
        let mut buf = vec![0u16; len + 1]; // 结尾 \0 由 COM 调用写入，初始全 0 即可
        names.GetString(index, &mut buf).ok()?;
        Some(String::from_utf16_lossy(&buf[..len]))
    }
}

/// 系统字体列表（去重、不区分大小写排序）
pub fn system_fonts() -> Vec<String> {
    let mut set = BTreeSet::new();
    unsafe {
        let Ok(factory) = DWriteCreateFactory::<windows::Win32::Graphics::DirectWrite::IDWriteFactory>(
            DWRITE_FACTORY_TYPE_SHARED,
        ) else {
            return Vec::new();
        };
        let mut collection = None;
        if factory
            .GetSystemFontCollection(&mut collection, false)
            .is_err()
            || collection.is_none()
        {
            return Vec::new();
        }
        let collection = collection.unwrap();

        let locale = user_locale();
        for i in 0..collection.GetFontFamilyCount() {
            let Ok(family) = collection.GetFontFamily(i) else {
                continue;
            };
            let Ok(names) = family.GetFamilyNames() else {
                continue;
            };
            if let Some(name) = family_name(&names, locale.as_deref()) {
                if !name.is_empty() {
                    set.insert(name);
                }
            }
        }
    }
    let mut fonts: Vec<String> = set.into_iter().collect();
    fonts.sort_by_key(|s| s.to_lowercase());
    fonts
}

#[cfg(test)]
mod tests {
    #[test]
    fn enumerate_contains_user_fonts() {
        let fonts = super::system_fonts();
        assert!(!fonts.is_empty(), "DirectWrite 字体集合不应为空");
        println!("共 {} 个字族：\n{:#?}", fonts.len(), fonts);
    }
}