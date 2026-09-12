//! WebDAV 凭证存取：密码存系统钥匙串（macOS Keychain / Windows Credential Manager / Linux Secret Service）。
//!
//! `sources.config`（JSON）只保存 username，不再保存明文密码；密码以
//! `webdav/{source_id}` 为条目名存入钥匙串（服务名 = 应用 identifier）。
//! 钥匙串不可用时由调用方回退为明文存储，保证功能可用。
//!
//! 读取侧带进程内缓存：媒体流每个 2MB 分块都会请求一次认证，每次都读系统钥匙串
//! 既慢，又存在「偶发读失败 → 请求退化成不带认证头」的风险。后者会被 OpenList 记为
//! 一次 WebDAV 登录失败（累计 5 次即封锁该 IP 5 分钟，且每个被挡请求都会续期），
//! 所以这里必须避免重复读取。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rusqlite::Connection;

use crate::error::{codes, err1};

const SERVICE: &str = "com.lanmusic.desktop";

fn cache() -> &'static Mutex<HashMap<i64, String>> {
    static CACHE: OnceLock<Mutex<HashMap<i64, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn entry(source_id: i64) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, &format!("webdav/{source_id}"))
        .map_err(|e| err1(codes::KEYRING_UNAVAILABLE, "error", e))
}

/// 写入密码；失败返回 Err（调用方应回退为明文存储）
pub fn set_password(source_id: i64, password: &str) -> Result<(), String> {
    entry(source_id)?
        .set_password(password)
        .map_err(|e| err1(codes::KEYRING_WRITE_FAILED, "error", e))?;
    if let Ok(mut c) = cache().lock() {
        c.insert(source_id, password.to_string());
    }
    Ok(())
}

/// 读取密码（无记录或钥匙串不可用时返回 None）
pub fn get_password(source_id: i64) -> Option<String> {
    if let Ok(c) = cache().lock() {
        if let Some(p) = c.get(&source_id) {
            return Some(p.clone());
        }
    }
    // 读失败不写缓存：下次调用会再试一次钥匙串，而不是把「没有密码」固化下来
    let password = entry(source_id).ok()?.get_password().ok()?;
    if let Ok(mut c) = cache().lock() {
        c.insert(source_id, password.clone());
    }
    Some(password)
}

/// 删除来源时清理钥匙串中的凭证
pub fn delete_password(source_id: i64) {
    if let Ok(mut c) = cache().lock() {
        c.remove(&source_id);
    }
    if let Ok(e) = entry(source_id) {
        // 无记录时会报错，忽略即可
        let _ = e.delete_credential();
    }
}

/// 一次性迁移：旧版本把密码明文存在 sources.config（JSON），迁移到钥匙串后从 config 移除。
/// 钥匙串不可用时保持原样（仍按明文工作），下次启动再尝试。
pub fn migrate_plaintext(conn: &Connection) {
    let rows: Vec<(i64, String)> = match conn
        .prepare("SELECT id, config FROM sources WHERE kind = 'webdav' AND config IS NOT NULL")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
                .map(|rows| rows.collect::<Result<Vec<_>, _>>())
        }) {
        Ok(Ok(rows)) => rows,
        _ => return,
    };
    for (id, config) in rows {
        let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&config) else {
            continue;
        };
        let Some(pw) = v
            .get("password")
            .and_then(|p| p.as_str())
            .map(str::to_string)
        else {
            continue;
        };
        if set_password(id, &pw).is_ok() {
            if let Some(obj) = v.as_object_mut() {
                obj.remove("password");
            }
            let _ = conn.execute(
                "UPDATE sources SET config = ?1 WHERE id = ?2",
                rusqlite::params![v.to_string(), id],
            );
        }
    }
}
