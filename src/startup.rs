//! 开机自启：注册表 Run + 启动文件夹；支持启用/禁用/删除与安全建议

use crate::operation_log;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use winreg::enums::*;
use winreg::{RegKey, RegValue};

const DISABLED_STORE: &str = r"Software\disk-janitor\DisabledStartup";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupKind {
    RegRun,
    FolderLnk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupAdvice {
    RecommendDisable,
    Optional,
    RecommendKeep,
    Unknown,
}

impl StartupAdvice {
    pub fn label(self) -> &'static str {
        match self {
            Self::RecommendDisable => "建议关闭",
            Self::Optional => "按需决定",
            Self::RecommendKeep => "建议保留",
            Self::Unknown => "未识别",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::RecommendDisable => 0,
            Self::Unknown => 1,
            Self::Optional => 2,
            Self::RecommendKeep => 3,
        }
    }

    pub fn can_disable(self) -> bool {
        matches!(self, Self::RecommendDisable | Self::Optional)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupImpact {
    High,
    Medium,
    Low,
    Unknown,
}

impl StartupImpact {
    pub fn label(self) -> &'static str {
        match self {
            Self::High => "潜在影响高",
            Self::Medium => "潜在影响中",
            Self::Low => "潜在影响低",
            Self::Unknown => "影响未知",
        }
    }
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
    /// 是否为本程序旧版本移入备份区的禁用项。
    pub stored_disabled: bool,
    pub purpose: String,
    pub advice: StartupAdvice,
    pub impact: StartupImpact,
    pub advice_reason: String,
}

pub fn list_startup_items() -> Vec<StartupItem> {
    let mut out = Vec::new();
    out.extend(list_reg_run(
        "HKCU",
        RegKey::predef(HKEY_CURRENT_USER),
        r"Software\Microsoft\Windows\CurrentVersion\Run",
    ));
    out.extend(list_reg_run(
        "HKLM",
        RegKey::predef(HKEY_LOCAL_MACHINE),
        r"Software\Microsoft\Windows\CurrentVersion\Run",
    ));
    out.extend(list_reg_run(
        "HKLM",
        RegKey::predef(HKEY_LOCAL_MACHINE),
        r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
    ));
    out.extend(list_disabled_reg());
    out.extend(list_startup_folder_items());
    out.sort_by(|a, b| {
        b.enabled
            .cmp(&a.enabled)
            .then_with(|| a.advice.rank().cmp(&b.advice.rank()))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

fn list_reg_run(hive: &'static str, root: RegKey, key_path: &str) -> Vec<StartupItem> {
    let mut out = Vec::new();
    let Ok(key) = root.open_subkey(key_path) else {
        return out;
    };
    for name in key.enum_values().filter_map(|r| r.ok()).map(|(n, _)| n) {
        if name.is_empty() {
            continue;
        }
        let command: String = key.get_value(&name).unwrap_or_default();
        let enabled = startup_approved_enabled(hive, key_path, &name);
        let (purpose, advice, impact, advice_reason) =
            assess_startup(&name, &command, StartupKind::RegRun);
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
            stored_disabled: false,
            purpose,
            advice,
            impact,
            advice_reason,
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
        let (purpose, advice, impact, advice_reason) =
            assess_startup(&value_name, &command, StartupKind::RegRun);
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
            stored_disabled: true,
            purpose,
            advice,
            impact,
            advice_reason,
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
            let command = path.display().to_string();
            let (purpose, advice, impact, advice_reason) =
                assess_startup(&name, &command, StartupKind::FolderLnk);
            out.push(StartupItem {
                name: name.trim_end_matches(".disabled").to_string(),
                command,
                location: label.to_string(),
                kind: StartupKind::FolderLnk,
                enabled: enabled_item,
                hive: "",
                reg_key: String::new(),
                value_name: String::new(),
                path,
                stored_disabled: false,
                purpose,
                advice,
                impact,
                advice_reason,
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
    if !item.advice.can_disable() {
        return Err("为保护系统，建议保留或未识别的启动项不允许在此禁用".into());
    }
    let result = match item.kind {
        StartupKind::RegRun => disable_reg(item),
        StartupKind::FolderLnk => disable_folder(item),
    };
    if result.is_ok() {
        operation_log::append(
            "startup-disable",
            &startup_log_target(item),
            "disabled",
            &item.command,
        );
    }
    result
}

pub fn enable_startup(item: &StartupItem) -> Result<(), String> {
    if item.enabled {
        return Err("已经是启用状态".into());
    }
    let result = match item.kind {
        StartupKind::RegRun => enable_reg(item),
        StartupKind::FolderLnk => enable_folder(item),
    };
    if result.is_ok() {
        operation_log::append(
            "startup-enable",
            &startup_log_target(item),
            "enabled",
            &item.command,
        );
    }
    result
}

pub fn delete_startup(item: &StartupItem) -> Result<(), String> {
    if item.enabled {
        return Err("为防止误删，启用中的启动项请先禁用，确认无影响后再删除".into());
    }
    if !item.advice.can_disable() {
        return Err("为保护系统，建议保留或未识别的启动项不允许删除".into());
    }
    match item.kind {
        StartupKind::RegRun => {
            if !item.stored_disabled {
                delete_reg_value(item.hive, &item.reg_key, &item.value_name)
            } else {
                // 删禁用存档
                let hkcu = RegKey::predef(HKEY_CURRENT_USER);
                let store = hkcu
                    .open_subkey_with_flags(DISABLED_STORE, KEY_ALL_ACCESS)
                    .map_err(|e| e.to_string())?;
                let id = item.path.to_string_lossy();
                store
                    .delete_subkey_all(id.as_ref())
                    .map_err(|e| e.to_string())
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

fn startup_log_target(item: &StartupItem) -> PathBuf {
    if item.kind == StartupKind::FolderLnk {
        item.path.clone()
    } else {
        PathBuf::from(format!(
            "{}\\{}\\{}",
            item.hive, item.reg_key, item.value_name
        ))
    }
}

fn disable_reg(item: &StartupItem) -> Result<(), String> {
    set_startup_approved(item.hive, &item.reg_key, &item.value_name, false)
}

fn enable_reg(item: &StartupItem) -> Result<(), String> {
    if item.stored_disabled {
        write_reg_command(item.hive, &item.reg_key, &item.value_name, &item.command)?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(store) = hkcu.open_subkey_with_flags(DISABLED_STORE, KEY_ALL_ACCESS) {
            let id = item.path.to_string_lossy();
            let _ = store.delete_subkey_all(id.as_ref());
        }
    }
    set_startup_approved(item.hive, &item.reg_key, &item.value_name, true)
}

fn startup_approved_key(reg_key: &str) -> &'static str {
    if reg_key.to_ascii_lowercase().contains("wow6432node") {
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32"
    } else {
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
    }
}

fn startup_approved_enabled(hive: &str, reg_key: &str, name: &str) -> bool {
    let root = if hive == "HKLM" {
        RegKey::predef(HKEY_LOCAL_MACHINE)
    } else {
        RegKey::predef(HKEY_CURRENT_USER)
    };
    let Ok(key) = root.open_subkey(startup_approved_key(reg_key)) else {
        return true;
    };
    let Ok(value) = key.get_raw_value(name) else {
        return true;
    };
    // Windows 当前使用 3 表示禁用；缺失及其它状态按启用处理。
    value.bytes.first().copied() != Some(3)
}

fn set_startup_approved(
    hive: &str,
    reg_key: &str,
    name: &str,
    enabled: bool,
) -> Result<(), String> {
    let root = if hive == "HKLM" {
        RegKey::predef(HKEY_LOCAL_MACHINE)
    } else {
        RegKey::predef(HKEY_CURRENT_USER)
    };
    let (key, _) = root
        .create_subkey(startup_approved_key(reg_key))
        .map_err(|e| format!("{e}（HKLM 通常需要管理员权限）"))?;
    let mut bytes = vec![0_u8; 12];
    bytes[0] = if enabled { 2 } else { 3 };
    if !enabled {
        let unix_100ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            / 100;
        let filetime = (unix_100ns as u64).saturating_add(116_444_736_000_000_000);
        bytes[4..12].copy_from_slice(&filetime.to_le_bytes());
    }
    key.set_raw_value(
        name,
        &RegValue {
            bytes,
            vtype: REG_BINARY,
        },
    )
    .map_err(|e| e.to_string())
}

fn assess_startup(
    name: &str,
    command: &str,
    kind: StartupKind,
) -> (String, StartupAdvice, StartupImpact, String) {
    let hay = format!("{name} {command}").to_ascii_lowercase();
    let result = if is_windows_system_command(command) {
        (
            "Windows 系统登录组件",
            StartupAdvice::RecommendKeep,
            StartupImpact::Low,
            "位于 Windows 系统目录，为避免影响系统功能，不提供关闭。",
        )
    } else if contains_any(&hay, &["securityhealth", "msascuil", "windows defender"]) {
        (
            "Windows 安全中心通知与防护状态",
            StartupAdvice::RecommendKeep,
            StartupImpact::Low,
            "关系到安全状态提醒，不建议关闭。",
        )
    } else if contains_any(&hay, &["ctfmon", "textinputhost"]) {
        (
            "Windows 输入法、语言栏和文本输入",
            StartupAdvice::RecommendKeep,
            StartupImpact::Low,
            "关闭后可能影响输入法切换和文本输入。",
        )
    } else if contains_any(&hay, &["rtkaud", "realtek", "audio service"]) {
        (
            "声卡控制、插孔检测和音效功能",
            StartupAdvice::RecommendKeep,
            StartupImpact::Low,
            "通常是硬件配套组件，保留更稳妥。",
        )
    } else if contains_any(
        &hay,
        &[
            "sunlogin",
            "awesun",
            "todesk",
            "teamviewer",
            "anydesk",
            "peanuthull",
            "hskddns",
            "phddns",
        ],
    ) {
        (
            "远程控制、内网穿透或动态域名后台",
            StartupAdvice::RecommendDisable,
            StartupImpact::Medium,
            "不需要无人值守或持续穿透时，建议关闭自启。",
        )
    } else if contains_any(
        &hay,
        &[
            "qq.exe", "qqnt", "weixin", "wechat", "wxwork", "dingtalk", "feishu", "lark",
            "discord", "skype", "teams",
        ],
    ) {
        (
            "聊天或协作软件后台",
            StartupAdvice::RecommendDisable,
            StartupImpact::High,
            "登录后再手动打开即可，常驻会增加启动负担。",
        )
    } else if contains_any(&hay, &["apifox", "postman"]) {
        (
            "接口开发工具的后台代理或快速启动组件",
            StartupAdvice::RecommendDisable,
            StartupImpact::Low,
            "不影响需要时手动打开主程序。",
        )
    } else if contains_any(&hay, &["wps", "ksolaunch"]) {
        (
            "办公软件更新、消息或预加载组件",
            StartupAdvice::RecommendDisable,
            StartupImpact::Low,
            "关闭自启通常不影响文档编辑，可在软件内更新。",
        )
    } else if contains_any(
        &hay,
        &[
            "steam",
            "epicgameslauncher",
            "battle.net",
            "eadesktop",
            "ubisoft",
            "spotify",
        ],
    ) {
        (
            "游戏商店、启动器或影音软件后台",
            StartupAdvice::RecommendDisable,
            StartupImpact::High,
            "通常无需随 Windows 启动，需要时手动打开即可。",
        )
    } else if contains_any(
        &hay,
        &["onedrive", "dropbox", "googledrive", "baidunetdisk"],
    ) {
        (
            "云盘文件同步",
            StartupAdvice::Optional,
            StartupImpact::Medium,
            "需要实时同步则保留，否则可关闭自启。",
        )
    } else if contains_any(&hay, &["clash", "v2ray", "sing-box", "tailscale"]) {
        (
            "网络代理或虚拟网络连接",
            StartupAdvice::Optional,
            StartupImpact::Medium,
            "依赖开机即联网时保留，否则可按需启动。",
        )
    } else if contains_any(&hay, &["everything"]) {
        (
            "本地文件快速搜索与索引",
            StartupAdvice::Optional,
            StartupImpact::Low,
            "经常使用秒搜则保留，不使用可关闭。",
        )
    } else if contains_any(&hay, &["docker", "mysql", "postgres", "sqlserver"]) {
        (
            "开发环境、容器或数据库后台",
            StartupAdvice::Optional,
            StartupImpact::High,
            "持续开发时保留；偶尔使用建议改为手动启动。",
        )
    } else if contains_any(&hay, &["intel", "nvidia", "amd", "igfx"]) {
        (
            "显卡驱动控制或辅助功能",
            StartupAdvice::Optional,
            StartupImpact::Low,
            "基础驱动通常不受影响，但控制面板附加功能可能受限。",
        )
    } else {
        (
            match kind {
                StartupKind::RegRun => "随用户登录启动的程序",
                StartupKind::FolderLnk => "启动文件夹中的快捷方式",
            },
            StartupAdvice::Unknown,
            StartupImpact::Unknown,
            "暂未识别该程序，请结合名称、发布者和命令路径判断。",
        )
    };
    (
        result.0.to_string(),
        result.1,
        result.2,
        result.3.to_string(),
    )
}

fn contains_any(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| hay.contains(needle))
}

fn is_windows_system_command(command: &str) -> bool {
    let command = command
        .trim()
        .trim_start_matches('"')
        .replace('/', "\\")
        .to_ascii_lowercase();
    command.starts_with(r"%windir%\")
        || command.starts_with(r"%systemroot%\")
        || command.starts_with(r"c:\windows\")
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
        .map(|a| PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs\Startup"))
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
        .map_err(|e| format!("{e}（HKLM 通常需要管理员权限）"))?;
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
        .map_err(|e| format!("{e}（HKLM 通常需要管理员权限）"))?;
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
        assert!(disabled_folder_dir()
            .to_string_lossy()
            .contains("disk-janitor"));
    }

    #[test]
    fn assesses_safe_and_optional_items() {
        let (_, advice, impact, _) = assess_startup(
            "SecurityHealth",
            "SecurityHealthSystray.exe",
            StartupKind::RegRun,
        );
        assert_eq!(advice, StartupAdvice::RecommendKeep);
        assert_eq!(impact, StartupImpact::Low);

        let (_, advice, impact, _) = assess_startup("MySQL80", "mysqld.exe", StartupKind::RegRun);
        assert_eq!(advice, StartupAdvice::Optional);
        assert_eq!(impact, StartupImpact::High);
    }

    #[test]
    fn recommends_chat_apps_off() {
        let (_, advice, impact, _) = assess_startup(
            "WXWork",
            r"D:\WXWork\WXWork.exe -autorun",
            StartupKind::RegRun,
        );
        assert_eq!(advice, StartupAdvice::RecommendDisable);
        assert_eq!(impact, StartupImpact::High);
    }

    #[test]
    fn protects_system_and_unknown_items() {
        let (_, advice, _, _) = assess_startup(
            "SystemComponent",
            r"C:\Windows\System32\component.exe",
            StartupKind::RegRun,
        );
        assert_eq!(advice, StartupAdvice::RecommendKeep);
        assert!(!advice.can_disable());

        let (_, advice, _, _) =
            assess_startup("Unrecognized", r"D:\Tools\custom.exe", StartupKind::RegRun);
        assert_eq!(advice, StartupAdvice::Unknown);
        assert!(!advice.can_disable());
    }
}
