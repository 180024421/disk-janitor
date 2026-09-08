//! 可安全提交给支持人员的脱敏诊断数据；不读取或导出任何文件内容。

use super::{atomic_write, ScanSnapshot};
use crate::model::ScanIndex;
use serde::{Deserialize, Serialize};
use std::path::Path;

const DIAGNOSTIC_ENTRY_LIMIT: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticEntry {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticScan {
    pub root: String,
    pub total_size: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub skipped: u64,
    pub skipped_bytes: u64,
    pub partial: bool,
    pub errors: Vec<String>,
    pub largest_entries: Vec<DiagnosticEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticSnapshot {
    pub root: String,
    pub saved_at: String,
    pub total_size: u64,
    pub file_count: u64,
    pub dir_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticBundle {
    pub schema_version: u32,
    pub generated_at: String,
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub scan: Option<DiagnosticScan>,
    pub snapshots: Vec<DiagnosticSnapshot>,
    pub messages: Vec<String>,
}

fn replace_case_insensitive(value: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return value.to_string();
    }
    // ASCII 大小写覆盖 Windows 盘符/Users；且不改变非 ASCII 字节长度，切片安全。
    let value_lower = value.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    let mut result = String::with_capacity(value.len());
    let mut cursor = 0;
    while let Some(offset) = value_lower[cursor..].find(&needle_lower) {
        let start = cursor + offset;
        result.push_str(&value[cursor..start]);
        result.push_str(replacement);
        cursor = start + needle.len();
    }
    result.push_str(&value[cursor..]);
    result
}

fn redact_with_profile(value: &str, profile: Option<&str>) -> String {
    let mut redacted = profile
        .filter(|profile| !profile.is_empty())
        .map(|profile| replace_case_insensitive(value, profile, "%USERPROFILE%"))
        .unwrap_or_else(|| value.to_string());

    // 环境变量不可用时，仍遮蔽常见 Windows 用户目录中的用户名段。
    let lower = redacted.to_ascii_lowercase();
    let windows_profile = lower
        .find(r":\users\")
        .map(|colon| (colon.saturating_sub(1), colon + 8));
    let unix_profile = lower.find("/users/").map(|start| (start, start + 7));
    if let Some((profile_start, name_start)) = windows_profile.or(unix_profile) {
        let name_end = redacted[name_start..]
            .find(['\\', '/'])
            .map(|offset| name_start + offset)
            .unwrap_or(redacted.len());
        if name_end > name_start {
            redacted.replace_range(profile_start..name_end, "%USERPROFILE%");
        }
    }
    redacted
}

/// 将 USERPROFILE（以及 HOME 兜底）替换为字面量 `%USERPROFILE%`。
pub fn redact_user_path(value: &str) -> String {
    let profile = std::env::var("USERPROFILE")
        .ok()
        .or_else(|| std::env::var("HOME").ok());
    redact_with_profile(value, profile.as_deref())
}

/// 构造只含路径元数据、计数与错误信息的诊断包，不读取文件内容。
pub fn build_diagnostics(
    index: Option<&ScanIndex>,
    snapshots: &[ScanSnapshot],
    messages: &[String],
) -> DiagnosticBundle {
    let scan = index.map(|index| {
        let mut largest_entries: Vec<DiagnosticEntry> = index
            .entries
            .values()
            .map(|entry| DiagnosticEntry {
                path: redact_user_path(&entry.path.display().to_string()),
                is_dir: entry.is_dir,
                size: entry.size,
            })
            .collect();
        largest_entries.sort_by(|a, b| b.size.cmp(&a.size));
        largest_entries.truncate(DIAGNOSTIC_ENTRY_LIMIT);
        DiagnosticScan {
            root: redact_user_path(&index.root.display().to_string()),
            total_size: index.get(&index.root).map(|entry| entry.size).unwrap_or(0),
            file_count: index.file_count,
            dir_count: index.dir_count,
            skipped: index.skipped,
            skipped_bytes: index.skipped_bytes,
            partial: index.partial,
            errors: index
                .errors
                .iter()
                .map(|error| redact_user_path(error))
                .collect(),
            largest_entries,
        }
    });
    DiagnosticBundle {
        schema_version: 1,
        generated_at: chrono::Local::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        scan,
        snapshots: snapshots
            .iter()
            .map(|snapshot| DiagnosticSnapshot {
                root: redact_user_path(&snapshot.root),
                saved_at: snapshot.saved_at.clone(),
                total_size: snapshot.total_size,
                file_count: snapshot.file_count,
                dir_count: snapshot.dir_count,
            })
            .collect(),
        messages: messages
            .iter()
            .map(|message| redact_user_path(message))
            .collect(),
    }
}

/// 以临时文件 + flush/sync + 原子替换导出 JSON 诊断包。
pub fn export_diagnostics(bundle: &DiagnosticBundle, path: &Path) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(bundle).map_err(|e| e.to_string())?;
    atomic_write(path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn redacts_profile_case_insensitively() {
        let value = r#"failed: C:\Users\Alice\Documents\private.txt"#;
        let redacted = redact_with_profile(value, Some(r"c:\users\alice"));
        assert_eq!(redacted, r#"failed: %USERPROFILE%\Documents\private.txt"#);
        assert!(!redacted.contains("Alice"));
    }

    #[test]
    fn diagnostic_export_is_atomic_and_contains_no_content_field() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("diagnostics.json");
        let bundle = DiagnosticBundle {
            schema_version: 1,
            generated_at: "now".into(),
            app_version: "test".into(),
            os: "test".into(),
            arch: "test".into(),
            scan: None,
            snapshots: Vec::new(),
            messages: vec!["safe".into()],
        };
        export_diagnostics(&bundle, &path).unwrap();
        let json = std::fs::read_to_string(path).unwrap();
        assert!(!json.contains("\"content\""));
        assert_eq!(
            serde_json::from_str::<DiagnosticBundle>(&json).unwrap(),
            bundle
        );
    }
}
