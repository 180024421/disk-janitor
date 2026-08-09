//! 开机自启：注册表 Run + 启动文件夹；支持启用/禁用/删除

use std::fs;
use std::path::PathBuf;
use winreg::enums::*;
use winreg::RegKey;

const DISABLED_STORE: &str = r"Software\disk-janitor\DisabledStartup";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupKind {
    RegRun,
    FolderLnk,
}

#[derive(Debug, Clone)]
pub struct StartupItem {
    pub name: String,
    pub command: String,
    pub location: String,
    pub kind: StartupKind,
    pub enabled: bool,
    /// 注册表：hive + 相对键路径；文件夹：.lnk 完整路径
    pub hive: &'static str,
    pub reg_key: String,
    pub value_name: String,
    pub path: PathBuf,
}

pub fn list_startup_items() -> Vec<StartupItem> {
    let mut out = Vec::new();
    out.extend(list_reg_run(
        "HKCU",
        RegKey::predef(HKEY_CURRENT_USER),
        r"Software\Microsoft\Windows\CurrentVersion\Run",
        true,
    ));
    out.extend(list_reg_run(
        "HKLM",
        RegKey::predef(HKEY_LOCAL_MACHINE),
        r"Software\Microsoft\Windows\CurrentVersion\Run",
        true,
    ));
    out.extend(list_reg_run(
        "HKLM",
        RegKey::predef(HKEY_LOCAL_MACHINE),
        r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
        true,
    ));
    out.extend(list_disabled_reg());
    out.extend(list_startup_folder_items());
    out.sort_by(|a, b| {
        b.enabled
            .cmp(&a.enabled)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

fn list_reg_run(
    hive: &'static str,
    root: RegKey,
    key_path: &str,
    enabled: bool,
) -> Vec<StartupItem> {
    let mut out = Vec::new();
    let Ok(key) = root.open_subkey(key_path) else {
        return out;
    };
    for name in key.enum_values().filter_map(|r| r.ok()).map(|(n, _)| n) {
        if name.is_empty() {
            continue;
        }
        let command: String = key.get_value(&name).unwrap_or_default();
        out.push(StartupItem {
            name: name.clone(),
            command,
            location: format!("{hive}\\{key_path}"),
            kind: StartupKind::RegRun,
            enabled,
            hive,
            reg_key: key_path.to_string(),
            value_name: name,
            path: PathBuf::new(),
        });
    }
    out
}

fn list_disabled_reg() -> Vec<StartupItem> {
    let mut out = Vec::new();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(store) = hkcu.open_subkey(DISABLED_STORE) else {
        return out;
    };
    for id in store.enum_keys().filter_map(|r| r.ok()) {
        let Ok(sub) = store.open_subkey(&id) else {
            continue;
        };
        let hive: String = sub.get_value("Hive").unwrap_or_default();
        let reg_key: String = sub.get_value("Key").unwrap_or_default();
        let value_name: String = sub.get_value("Name").unwrap_or_default();
        let command: String = sub.get_value("Command").unwrap_or_default();
        if value_name.is_empty() {
            continue;
        }
        let hive_static: &'static str = if hive.eq_ignore_ascii_case("HKLM") {
            "HKLM"
        } else {
            "HKCU"
        };
        out.push(StartupItem {
            name: value_name.clone(),
            command,
            location: format!("已禁用 · {hive_static}\\{reg_key}"),
            kind: StartupKind::RegRun,
            enabled: false,
            hive: hive_static,
            reg_key,
            value_name,
            path: PathBuf::from(id), // 存禁用项子键名
        });
    }
    out
}

fn list_startup_folder_items() -> Vec<StartupItem> {
    let mut out = Vec::new();
    for (label, dir, enabled) in startup_dirs() {
        if !dir.is_dir() {
            continue;
        }
        let Ok(rd) = fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            let path = ent.path();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let is_lnk = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("lnk"))
                .unwrap_or(false);
            let is_disabled = name.to_ascii_lowercase().ends_with(".lnk.disabled")
                || name.to_ascii_lowercase().ends_with(".disabled");
            if !is_lnk && !is_disabled {
                // 也接受 .lnk.disabled
                if !name.to_ascii_lowercase().contains(".lnk") {
                    continue;
                }
            }
            let enabled_item = enabled && !is_disabled;
            out.push(StartupItem {
                name: name.trim_end_matches(".disabled").to_string(),
                command: path.display().to_string(),
                location: label.to_string(),
                kind: StartupKind::FolderLnk,
                enabled: enabled_item,
                hive: "",
                reg_key: String::new(),
                value_name: String::new(),
                path,
            });
        }
    }
    out
}

fn startup_dirs() -> Vec<(&'static str, PathBuf, bool)> {
    let mut v = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        v.push((
            "用户启动文件夹",
            PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\Startup"),
            true,
        ));
    }
    if let Ok(programdata) = std::env::var("ProgramData") {
        v.push((
            "公用启动文件夹",
            PathBuf::from(programdata).join(r"Microsoft\Windows\Start Menu\Programs\Startup"),
            true,
        ));
    }
    // 我们自己的禁用目录
    v.push(("已禁用 · 启动文件夹备份", disabled_folder_dir(), false));
    v
}

fn disabled_folder_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("disk-janitor").join("disabled-startup")
}

pub fn disable_startup(item: &StartupItem) -> Result<(), String> {
    if !item.enabled {
        return Err("已经是禁用状态".into());
    }
    match item.kind {
        StartupKind::RegRun => disable_reg(item),
        StartupKind::FolderLnk => disable_folder(item),
    }
}

pub fn enable_startup(item: &StartupItem) -> Result<(), String> {
    if item.enabled {
        return Err("已经是启用状态".into());
    }
    match item.kind {
        StartupKind::RegRun => enable_reg(item),
        StartupKind::FolderLnk => enable_folder(item),
    }
}

pub fn delete_startup(item: &StartupItem) -> Result<(), String> {
    match item.kind {
        StartupKind::RegRun => {
            if item.enabled {
                delete_reg_value(item.hive, &item.reg_key, &item.value_name)
            } else {
                // 删禁用存档
                let hkcu = RegKey::predef(HKEY_CURRENT_USER);
                let store = hkcu
                    .open_subkey_with_flags(DISABLED_STORE, KEY_ALL_ACCESS)
                    .map_err(|e| e.to_string())?;
                let id = item.path.to_string_lossy();
                store.delete_subkey_all(id.as_ref()).map_err(|e| e.to_string())
            }
        }
        StartupKind::FolderLnk => {
            if item.path.exists() {
                trash::delete(&item.path).map_err(|e| e.to_string())
            } else {
                Ok(())
            }
        }
    }
}

fn disable_reg(item: &StartupItem) -> Result<(), String> {
    let command = read_reg_command(item.hive, &item.reg_key, &item.value_name)?;
    // 写入禁用存档
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (store, _) = hkcu
        .create_subkey(DISABLED_STORE)
        .map_err(|e| e.to_string())?;
    let id = format!(
        "{}_{}",
        item.hive,
        item.value_name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
    );
    let (sub, _) = store.create_subkey(&id).map_err(|e| e.to_string())?;
    sub.set_value("Hive", &item.hive).map_err(|e| e.to_string())?;
    sub.set_value("Key", &item.reg_key)
        .map_err(|e| e.to_string())?;
    sub.set_value("Name", &item.value_name)
        .map_err(|e| e.to_string())?;
    sub.set_value("Command", &command)
        .map_err(|e| e.to_string())?;
    // 从 Run 删除
    delete_reg_value(item.hive, &item.reg_key, &item.value_name)?;
    Ok(())
}

fn enable_reg(item: &StartupItem) -> Result<(), String> {
    let command = item.command.clone();
    write_reg_command(item.hive, &item.reg_key, &item.value_name, &command)?;
    // 删禁用存档
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(store) = hkcu.open_subkey_with_flags(DISABLED_STORE, KEY_ALL_ACCESS) {
        let id = item.path.to_string_lossy();
        let _ = store.delete_subkey_all(id.as_ref());
    }
    Ok(())
}

fn disable_folder(item: &StartupItem) -> Result<(), String> {
    let dest_dir = disabled_folder_dir();
    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let file_name = item
        .path
        .file_name()
        .ok_or_else(|| "无效路径".to_string())?;
    let dest = dest_dir.join(file_name);
    fs::rename(&item.path, &dest).map_err(|e| e.to_string())
}

fn enable_folder(item: &StartupItem) -> Result<(), String> {
    let user_startup = std::env::var("APPDATA")
        .map(|a| {
            PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs\Startup")
        })
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(&user_startup).map_err(|e| e.to_string())?;
    let file_name = item
        .path
        .file_name()
        .ok_or_else(|| "无效路径".to_string())?;
    let dest = user_startup.join(file_name);
    fs::rename(&item.path, &dest).map_err(|e| e.to_string())
}

fn read_reg_command(hive: &str, key_path: &str, name: &str) -> Result<String, String> {
    let root = match hive {
        "HKLM" => RegKey::predef(HKEY_LOCAL_MACHINE),
        _ => RegKey::predef(HKEY_CURRENT_USER),
    };
    let key = root.open_subkey(key_path).map_err(|e| e.to_string())?;
    key.get_value::<String, _>(name).map_err(|e| e.to_string())
}

fn write_reg_command(hive: &str, key_path: &str, name: &str, command: &str) -> Result<(), String> {
    let root = match hive {
        "HKLM" => RegKey::predef(HKEY_LOCAL_MACHINE),
        _ => RegKey::predef(HKEY_CURRENT_USER),
    };
    let key = root
        .open_subkey_with_flags(key_path, KEY_SET_VALUE)
        .map_err(|e| {
            format!("{e}（HKLM 通常需要管理员权限）")
        })?;
    key.set_value(name, &command.to_string())
        .map_err(|e| e.to_string())
}

fn delete_reg_value(hive: &str, key_path: &str, name: &str) -> Result<(), String> {
    let root = match hive {
        "HKLM" => RegKey::predef(HKEY_LOCAL_MACHINE),
        _ => RegKey::predef(HKEY_CURRENT_USER),
    };
    let key = root
        .open_subkey_with_flags(key_path, KEY_SET_VALUE)
        .map_err(|e| {
            format!("{e}（HKLM 通常需要管理员权限）")
        })?;
    key.delete_value(name).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_runs() {
        let items = list_startup_items();
        // 不保证非空，但不 panic
        let _ = items.len();
    }

    #[test]
    fn disabled_dir_path() {
        assert!(disabled_folder_dir().to_string_lossy().contains("disk-janitor"));
    }
}
