//! 无效「卸载」注册表项（安装目录 / 卸载程序已不存在）

use crate::operation_log;
use crate::software::{expand_env, extract_exe_path, path_missing};
use std::path::PathBuf;
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;

#[derive(Debug, Clone)]
pub struct OrphanReg {
    pub display_name: String,
    pub reason: String,
    pub hive: &'static str,
    /// 父键相对路径，如 SOFTWARE\...\Uninstall
    pub parent_path: String,
    /// 子键名
    pub key_name: String,
    pub full_path: String,
    pub selected: bool,
}

pub fn scan_orphan_uninstall_keys() -> Vec<OrphanReg> {
    let mut out = Vec::new();
    let roots: [(&'static str, RegKey, &str); 4] = [
        (
            "HKLM",
            RegKey::predef(HKEY_LOCAL_MACHINE),
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            "HKLM",
            RegKey::predef(HKEY_LOCAL_MACHINE),
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            "HKCU",
            RegKey::predef(HKEY_CURRENT_USER),
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            "HKCU",
            RegKey::predef(HKEY_CURRENT_USER),
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
    ];

    for (hive, root, parent) in roots {
        let Ok(key) = root.open_subkey(parent) else {
            continue;
        };
        for name in key.enum_keys().filter_map(|r| r.ok()) {
            let Ok(sub) = key.open_subkey(&name) else {
                continue;
            };
            if let Some(o) = classify_orphan(hive, parent, &name, &sub) {
                out.push(o);
            }
        }
    }

    out.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
    });
    out
}

fn classify_orphan(
    hive: &'static str,
    parent: &str,
    name: &str,
    sub: &RegKey,
) -> Option<OrphanReg> {
    let display_name: String = sub
        .get_value("DisplayName")
        .unwrap_or_else(|_| name.to_string());
    let display_name = if display_name.trim().is_empty() {
        name.to_string()
    } else {
        display_name.trim().to_string()
    };

    let system_component: u32 = sub.get_value("SystemComponent").unwrap_or(0);
    if system_component == 1 {
        return None;
    }
    if name.starts_with("KB") {
        return None;
    }

    let install_location: String = sub.get_value("InstallLocation").unwrap_or_default();
    let uninstall: String = sub.get_value("UninstallString").unwrap_or_default();
    let icon: String = sub.get_value("DisplayIcon").unwrap_or_default();

    let mut reasons = Vec::new();

    let loc = install_location.trim();
    if !loc.is_empty() {
        let p = PathBuf::from(expand_env(loc));
        if path_missing(&p) {
            reasons.push(format!("安装目录不存在: {}", p.display()));
        }
    }

    if let Some(exe) = extract_exe_path(&uninstall) {
        if path_missing(&exe) {
            reasons.push(format!("卸载程序不存在: {}", exe.display()));
        }
    }

    if let Some(exe) = extract_exe_path(&icon) {
        // 仅当已有其它失效迹象时，图标失效作为补充；单独图标缺失不够（可指向共享 dll）
        if path_missing(&exe) && !reasons.is_empty() {
            reasons.push(format!("图标路径不存在: {}", exe.display()));
        }
    }

    // 无 InstallLocation 时：卸载 exe 明确缺失才算残留
    if reasons.is_empty() {
        return None;
    }

    // 若仅有安装目录缺失、但卸载程序仍在，可能是便携/自定义布局 — 仍标为候选，UI 默认不勾选
    let only_location = reasons.len() == 1 && reasons[0].starts_with("安装目录");
    let selected = !only_location;

    Some(OrphanReg {
        display_name,
        reason: reasons.join("；"),
        hive,
        parent_path: parent.to_string(),
        key_name: name.to_string(),
        full_path: format!("{hive}\\{parent}\\{name}"),
        selected,
    })
}

pub fn delete_orphan_keys(items: &[OrphanReg]) -> (usize, Vec<String>) {
    let mut ok = 0usize;
    let mut errs = Vec::new();
    let backup_dir = recovery_dir();
    if let Err(e) = std::fs::create_dir_all(&backup_dir) {
        return (0, vec![format!("无法创建注册表恢复目录：{e}")]);
    }
    for (index, item) in items.iter().enumerate() {
        let backup = backup_dir.join(format!(
            "{index:03}-{}.reg",
            safe_file_name(&item.display_name)
        ));
        if let Err(e) = export_registry_key(&item.full_path, &backup) {
            errs.push(format!("{}: 备份失败，已取消删除：{e}", item.full_path));
            operation_log::append(
                "registry-delete",
                &PathBuf::from(&item.full_path),
                "blocked",
                "注册表导出失败",
            );
            continue;
        }
        let hkey = match item.hive {
            "HKLM" => HKEY_LOCAL_MACHINE,
            "HKCU" => HKEY_CURRENT_USER,
            _ => {
                errs.push(format!("{}: 未知 hive", item.full_path));
                continue;
            }
        };
        let root = RegKey::predef(hkey);
        match root.open_subkey_with_flags(&item.parent_path, KEY_ALL_ACCESS) {
            Ok(parent) => match parent.delete_subkey_all(&item.key_name) {
                Ok(()) => {
                    ok += 1;
                    operation_log::append(
                        "registry-delete",
                        &PathBuf::from(&item.full_path),
                        "backed-up-and-deleted",
                        &backup.display().to_string(),
                    );
                }
                Err(e) => errs.push(format!(
                    "{}: {}（HKLM 项通常需要管理员权限）",
                    item.full_path, e
                )),
            },
            Err(e) => errs.push(format!("{}: 打开失败 {}", item.full_path, e)),
        }
    }
    (ok, errs)
}

fn recovery_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("disk-janitor")
        .join("recovery")
        .join(chrono::Local::now().format("%Y%m%d-%H%M%S").to_string())
}

fn safe_file_name(name: &str) -> String {
    let value: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    if value.is_empty() {
        "registry".into()
    } else {
        value
    }
}

fn export_registry_key(full_path: &str, destination: &std::path::Path) -> Result<(), String> {
    let output = Command::new("reg.exe")
        .args(["export", full_path, &destination.to_string_lossy(), "/y"])
        .output()
        .map_err(|e| format!("无法调用 reg.exe：{e}"))?;
    if output.status.success() && destination.is_file() {
        Ok(())
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        Err(if error.trim().is_empty() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            error.trim().to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_runs() {
        // 仅确保不 panic
        let _ = scan_orphan_uninstall_keys();
    }
}
