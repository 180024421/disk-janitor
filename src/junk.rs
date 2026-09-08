//! 常见垃圾规则建议

use crate::model::{format_bytes, is_sensitive_path, EstimateQuality};
use crate::scan::quick_dir_size_with_status;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JunkSource {
    Windows,
    Browser,
    DeveloperTool,
    Communication,
    Office,
    GamePlatform,
    UserFiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rebuildability {
    Rebuildable,
    PartiallyRebuildable,
    NotRebuildable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserContentRisk {
    None,
    Possible,
    ContainsUserContent,
}

#[derive(Debug, Clone)]
pub struct JunkRule {
    pub id: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    pub sensitive: bool,
    /// 默认是否勾选
    pub default_selected: bool,
    pub source: JunkSource,
    pub rebuildability: Rebuildability,
    pub user_content_risk: UserContentRisk,
    /// 只把达到此年龄的文件交给清理层；None 表示不按年龄过滤。
    pub min_age_days: Option<u32>,
    /// 扫描可以在线进行，但清理前应关闭对应进程。
    pub requires_process_exit: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgePreview {
    pub total_files: u64,
    pub total_size: u64,
    pub older_than_7_days_files: u64,
    pub older_than_7_days_size: u64,
    pub older_than_30_days_files: u64,
    pub older_than_30_days_size: u64,
    pub older_than_90_days_files: u64,
    pub older_than_90_days_size: u64,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct JunkHit {
    pub rule_id: String,
    pub title: String,
    pub detail: String,
    pub paths: Vec<PathBuf>,
    pub size: u64,
    pub sensitive: bool,
    pub selected: bool,
    pub note: String,
    pub quality: EstimateQuality,
    pub source: Option<JunkSource>,
    pub rebuildability: Option<Rebuildability>,
    pub user_content_risk: Option<UserContentRisk>,
    pub min_age_days: Option<u32>,
    pub requires_process_exit: bool,
    pub age_preview: AgePreview,
}

macro_rules! rule {
    ($id:literal, $title:literal, $detail:literal, $sensitive:expr, $selected:expr,
     $source:expr, $rebuild:expr, $risk:expr, $age:expr, $close:expr) => {
        JunkRule {
            id: $id,
            title: $title,
            detail: $detail,
            sensitive: $sensitive,
            default_selected: $selected,
            source: $source,
            rebuildability: $rebuild,
            user_content_risk: $risk,
            min_age_days: $age,
            requires_process_exit: $close,
        }
    };
}

const RULES: &[JunkRule] = &[
    rule!(
        "user_temp",
        "用户临时文件",
        "%TEMP% / Local\\Temp",
        false,
        true,
        JunkSource::Windows,
        Rebuildability::Rebuildable,
        UserContentRisk::Possible,
        Some(7),
        false
    ),
    rule!(
        "win_temp",
        "Windows\\Temp",
        "系统临时目录（可能需管理员）",
        true,
        false,
        JunkSource::Windows,
        Rebuildability::Rebuildable,
        UserContentRisk::Possible,
        Some(7),
        false
    ),
    rule!(
        "prefetch",
        "Prefetch",
        "预读取缓存（清理后可能拖慢开机，默认不勾选）",
        true,
        false,
        JunkSource::Windows,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        false
    ),
    rule!(
        "thumbcache",
        "缩略图缓存",
        "Explorer 缩略图数据库",
        false,
        true,
        JunkSource::Windows,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "delivery_opt",
        "传递优化缓存",
        "Windows Delivery Optimization",
        true,
        false,
        JunkSource::Windows,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "chrome_cache",
        "Chrome 缓存",
        "Cache / Code Cache / GPUCache（Default）",
        false,
        true,
        JunkSource::Browser,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "edge_cache",
        "Edge 缓存",
        "Cache / Code Cache / GPUCache（Default）",
        false,
        true,
        JunkSource::Browser,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "firefox_cache",
        "Firefox 缓存",
        "Local\\Mozilla\\Firefox\\Profiles\\*\\cache2",
        false,
        true,
        JunkSource::Browser,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "npm_cache",
        "npm 缓存",
        "%LOCALAPPDATA%\\npm-cache",
        false,
        false,
        JunkSource::DeveloperTool,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "pip_cache",
        "pip 缓存",
        "Local\\pip\\Cache",
        false,
        false,
        JunkSource::DeveloperTool,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "cargo_cache",
        "Cargo 注册表缓存",
        "%USERPROFILE%\\.cargo\\registry\\cache",
        false,
        false,
        JunkSource::DeveloperTool,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "downloads_large_old",
        "下载目录：大而旧的文件",
        "Downloads 中 >100MB 且超过 90 天（含子文件夹）",
        false,
        false,
        JunkSource::UserFiles,
        Rebuildability::NotRebuildable,
        UserContentRisk::ContainsUserContent,
        Some(90),
        false
    ),
    rule!(
        "win_update_download",
        "Windows Update 下载缓存",
        "SoftwareDistribution\\Download（敏感，默认不勾选）",
        true,
        false,
        JunkSource::Windows,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "chrome_cache_profiles",
        "Chrome 其他配置缓存",
        "User Data\\Profile *\\Cache 等（默认不勾选）",
        false,
        false,
        JunkSource::Browser,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "edge_profile_cache",
        "Edge 其他配置缓存",
        "User Data\\Profile *\\Cache 等（默认不勾选）",
        false,
        false,
        JunkSource::Browser,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "windows_old",
        "Windows.old",
        "系统盘根目录 Windows.old（升级残留，体积大）",
        true,
        false,
        JunkSource::Windows,
        Rebuildability::NotRebuildable,
        UserContentRisk::ContainsUserContent,
        None,
        false
    ),
    rule!(
        "cbs_logs",
        "CBS 日志",
        "Windows\\Logs\\CBS",
        true,
        false,
        JunkSource::Windows,
        Rebuildability::NotRebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "font_cache",
        "字体缓存",
        "Local\\FontCache / Service\\FontCache",
        false,
        false,
        JunkSource::Windows,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "directx_shader",
        "DirectX 着色器缓存",
        "D3DSCache / DXCache",
        false,
        true,
        JunkSource::Windows,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        None,
        true
    ),
    rule!(
        "edge_cookies",
        "隐私·Edge Cookies（默认不勾选）",
        "User Data\\Default\\Network\\Cookies",
        true,
        false,
        JunkSource::Browser,
        Rebuildability::NotRebuildable,
        UserContentRisk::ContainsUserContent,
        None,
        true
    ),
    rule!(
        "chrome_cookies",
        "隐私·Chrome Cookies（默认不勾选）",
        "User Data\\Default\\Network\\Cookies",
        true,
        false,
        JunkSource::Browser,
        Rebuildability::NotRebuildable,
        UserContentRisk::ContainsUserContent,
        None,
        true
    ),
    rule!(
        "edge_history",
        "隐私·Edge 历史（默认不勾选）",
        "User Data\\Default\\History",
        true,
        false,
        JunkSource::Browser,
        Rebuildability::NotRebuildable,
        UserContentRisk::ContainsUserContent,
        None,
        true
    ),
    rule!(
        "chrome_history",
        "隐私·Chrome 历史（默认不勾选）",
        "User Data\\Default\\History",
        true,
        false,
        JunkSource::Browser,
        Rebuildability::NotRebuildable,
        UserContentRisk::ContainsUserContent,
        None,
        true
    ),
    rule!(
        "wechat_cache",
        "微信可重建缓存",
        "WeChat/Weixin 已知 Cache、Code Cache、GPUCache",
        true,
        false,
        JunkSource::Communication,
        Rebuildability::PartiallyRebuildable,
        UserContentRisk::Possible,
        Some(7),
        true
    ),
    rule!(
        "qq_cache",
        "QQ 可重建缓存",
        "QQ/QQNT 已知 Cache、Code Cache、GPUCache",
        true,
        false,
        JunkSource::Communication,
        Rebuildability::PartiallyRebuildable,
        UserContentRisk::Possible,
        Some(7),
        true
    ),
    rule!(
        "wps_cache",
        "WPS 可重建缓存",
        "Kingsoft/WPS 已知 cache/temp 目录",
        true,
        false,
        JunkSource::Office,
        Rebuildability::PartiallyRebuildable,
        UserContentRisk::Possible,
        Some(7),
        true
    ),
    rule!(
        "vscode_cache",
        "VS Code 可重建缓存",
        "Cache、CachedData、Code Cache、GPUCache",
        false,
        false,
        JunkSource::DeveloperTool,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        Some(7),
        true
    ),
    rule!(
        "jetbrains_cache",
        "JetBrains IDE 缓存",
        "Local\\JetBrains\\*\\caches",
        false,
        false,
        JunkSource::DeveloperTool,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        Some(7),
        true
    ),
    rule!(
        "steam_cache",
        "Steam 网页缓存",
        "appcache\\httpcache / htmlcache",
        false,
        false,
        JunkSource::GamePlatform,
        Rebuildability::Rebuildable,
        UserContentRisk::None,
        Some(7),
        true
    ),
];

pub fn builtin_junk_rules() -> &'static [JunkRule] {
    RULES
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).map(PathBuf::from)
}

fn browser_cache_names() -> &'static [&'static str] {
    &["Cache", "Code Cache", "GPUCache", "Service Worker"]
}

fn push_browser_profile_caches(out: &mut Vec<PathBuf>, profile_dir: &Path) {
    for name in browser_cache_names() {
        let p = profile_dir.join(name);
        if p.exists() {
            out.push(p);
        }
    }
}

fn browser_cache_dirs(product: &str, company: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Some(local) = env_path("LOCALAPPDATA") else {
        return out;
    };
    let user_data = local.join(company).join(product).join("User Data");
    push_browser_profile_caches(&mut out, &user_data.join("Default"));
    out
}

fn browser_profile_cache_dirs(product: &str, company: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Some(local) = env_path("LOCALAPPDATA") else {
        return out;
    };
    let user_data = local.join(company).join(product).join("User Data");
    if let Ok(rd) = std::fs::read_dir(&user_data) {
        for ent in rd.flatten() {
            let name = ent.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("Profile ") {
                push_browser_profile_caches(&mut out, &ent.path());
            }
        }
    }
    out
}

fn push_known_children(out: &mut Vec<PathBuf>, base: PathBuf, children: &[&str]) {
    for child in children {
        out.push(base.join(child));
    }
}

fn push_product_caches(out: &mut Vec<PathBuf>, root: &Path, products: &[&str]) {
    const CACHE_NAMES: &[&str] = &["Cache", "Code Cache", "GPUCache"];
    for product in products {
        push_known_children(out, root.join(product), CACHE_NAMES);
    }
}

fn jetbrains_cache_dirs(local: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let root = local.join("JetBrains");
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.push(path.join("caches"));
            }
        }
    }
    out
}

fn rule_paths(id: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    match id {
        "user_temp" => {
            if let Some(t) = env_path("TEMP") {
                out.push(t);
            }
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.push(local.join("Temp"));
            }
        }
        "win_temp" => out.push(PathBuf::from(r"C:\Windows\Temp")),
        "prefetch" => out.push(PathBuf::from(r"C:\Windows\Prefetch")),
        "thumbcache" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.push(local.join("Microsoft").join("Windows").join("Explorer"));
            }
        }
        "delivery_opt" => {
            out.push(PathBuf::from(
                r"C:\Windows\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache",
            ));
        }
        "win_update_download" => {
            if let Some(win) = std::env::var_os("SystemRoot") {
                out.push(
                    PathBuf::from(win)
                        .join("SoftwareDistribution")
                        .join("Download"),
                );
            } else {
                out.push(PathBuf::from(r"C:\Windows\SoftwareDistribution\Download"));
            }
        }
        "chrome_cache" => out.extend(browser_cache_dirs("Chrome", "Google")),
        "edge_cache" => out.extend(browser_cache_dirs("Edge", "Microsoft")),
        "chrome_cache_profiles" => out.extend(browser_profile_cache_dirs("Chrome", "Google")),
        "edge_profile_cache" => out.extend(browser_profile_cache_dirs("Edge", "Microsoft")),
        "firefox_cache" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                let profiles = local.join("Mozilla").join("Firefox").join("Profiles");
                if let Ok(rd) = std::fs::read_dir(&profiles) {
                    for ent in rd.flatten() {
                        let cache2 = ent.path().join("cache2");
                        if cache2.exists() {
                            out.push(cache2);
                        }
                    }
                }
            }
        }
        "npm_cache" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.push(local.join("npm-cache"));
            }
        }
        "pip_cache" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.push(local.join("pip").join("Cache"));
            }
        }
        "cargo_cache" => {
            if let Some(up) = env_path("USERPROFILE") {
                out.push(up.join(".cargo").join("registry").join("cache"));
            }
        }
        "downloads_large_old" => {
            if let Some(user) = env_path("USERPROFILE") {
                out.push(user.join("Downloads"));
            }
        }
        "windows_old" => {
            for letter in [b'C', b'D', b'E'] {
                let p = PathBuf::from(format!("{}:\\Windows.old", letter as char));
                if p.exists() {
                    out.push(p);
                }
            }
        }
        "cbs_logs" => {
            if let Some(win) = std::env::var_os("SystemRoot") {
                out.push(PathBuf::from(win).join("Logs").join("CBS"));
            } else {
                out.push(PathBuf::from(r"C:\Windows\Logs\CBS"));
            }
        }
        "font_cache" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.push(local.join("FontCache"));
                out.push(
                    local
                        .join("Microsoft")
                        .join("Windows")
                        .join("Fonts")
                        .join("FontCache"),
                );
            }
            out.push(PathBuf::from(
                r"C:\Windows\ServiceProfiles\LocalService\AppData\Local\FontCache",
            ));
        }
        "directx_shader" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.push(local.join("D3DSCache"));
                out.push(local.join("Microsoft").join("Windows").join("DXCache"));
            }
        }
        "edge_cookies" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.push(
                    local
                        .join("Microsoft")
                        .join("Edge")
                        .join("User Data")
                        .join("Default")
                        .join("Network")
                        .join("Cookies"),
                );
            }
        }
        "chrome_cookies" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.push(
                    local
                        .join("Google")
                        .join("Chrome")
                        .join("User Data")
                        .join("Default")
                        .join("Network")
                        .join("Cookies"),
                );
            }
        }
        "edge_history" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.push(
                    local
                        .join("Microsoft")
                        .join("Edge")
                        .join("User Data")
                        .join("Default")
                        .join("History"),
                );
            }
        }
        "chrome_history" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.push(
                    local
                        .join("Google")
                        .join("Chrome")
                        .join("User Data")
                        .join("Default")
                        .join("History"),
                );
            }
        }
        "wechat_cache" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                push_product_caches(
                    &mut out,
                    &local.join("Tencent"),
                    &["WeChat", "WeChatAppEx", "Weixin"],
                );
            }
            if let Some(roaming) = env_path("APPDATA") {
                push_product_caches(&mut out, &roaming.join("Tencent"), &["WeChat", "Weixin"]);
            }
        }
        "qq_cache" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                push_product_caches(&mut out, &local.join("Tencent"), &["QQ", "QQNT"]);
            }
            if let Some(roaming) = env_path("APPDATA") {
                push_product_caches(&mut out, &roaming.join("Tencent"), &["QQ", "QQNT"]);
            }
        }
        "wps_cache" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                push_known_children(
                    &mut out,
                    local.join("Kingsoft").join("WPS Office"),
                    &["cache", "temp"],
                );
            }
            if let Some(roaming) = env_path("APPDATA") {
                push_known_children(
                    &mut out,
                    roaming.join("kingsoft").join("office6"),
                    &["cache", "temp"],
                );
            }
        }
        "vscode_cache" => {
            if let Some(roaming) = env_path("APPDATA") {
                push_known_children(
                    &mut out,
                    roaming.join("Code"),
                    &["Cache", "CachedData", "Code Cache", "GPUCache"],
                );
            }
        }
        "jetbrains_cache" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.extend(jetbrains_cache_dirs(&local));
            }
        }
        "steam_cache" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.push(local.join("Steam").join("htmlcache"));
            }
            if let Some(program_files) = env_path("ProgramFiles(x86)") {
                out.push(
                    program_files
                        .join("Steam")
                        .join("appcache")
                        .join("httpcache"),
                );
            }
        }
        _ => {}
    }
    let mut seen = std::collections::HashSet::new();
    out.into_iter()
        .filter(|p| p.exists())
        .filter(|p| seen.insert(p.to_string_lossy().to_ascii_lowercase()))
        .collect()
}

fn collect_large_old_files(dir: &Path, cancel: &AtomicBool) -> (Vec<PathBuf>, u64, bool) {
    let mut paths = Vec::new();
    let mut size = 0u64;
    let now = SystemTime::now();
    let max_age = Duration::from_secs(90 * 24 * 3600);
    let min_size = 100u64 * 1024 * 1024;
    let mut stack = vec![dir.to_path_buf()];
    let mut visited = 0u64;
    let mut complete = true;
    while let Some(d) = stack.pop() {
        if cancel.load(Ordering::Relaxed) || visited > 20_000 {
            complete = false;
            break;
        }
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for ent in rd.flatten() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            visited += 1;
            let p = ent.path();
            let Ok(meta) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                // 避免扫太深
                if stack.len() < 400 {
                    stack.push(p);
                }
                continue;
            }
            if meta.len() < min_size {
                continue;
            }
            let Ok(mtime) = meta.modified() else {
                continue;
            };
            let Ok(age) = now.duration_since(mtime) else {
                continue;
            };
            if age >= max_age {
                size += meta.len();
                paths.push(p);
            }
        }
    }
    (paths, size, complete)
}

fn collect_thumbcache_files(dir: &Path) -> (Vec<PathBuf>, u64) {
    let mut paths = Vec::new();
    let mut size = 0u64;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return (paths, size);
    };
    for ent in rd.flatten() {
        let p = ent.path();
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.starts_with("thumbcache_") && name.ends_with(".db") {
            if let Ok(meta) = std::fs::metadata(&p) {
                size += meta.len();
                paths.push(p);
            }
        }
    }
    (paths, size)
}

fn collect_old_temp_files(
    dir: &Path,
    cancel: &AtomicBool,
    min_age: Duration,
    max_files: usize,
) -> (Vec<PathBuf>, u64, bool) {
    let mut paths = Vec::new();
    let mut size = 0_u64;
    let mut stack = vec![dir.to_path_buf()];
    let now = SystemTime::now();
    let mut complete = true;
    while let Some(current) = stack.pop() {
        if cancel.load(Ordering::Relaxed) || paths.len() >= max_files {
            complete = false;
            break;
        }
        let Ok(entries) = std::fs::read_dir(current) else {
            continue;
        };
        for entry in entries.flatten() {
            if cancel.load(Ordering::Relaxed) || paths.len() >= max_files {
                complete = false;
                break;
            }
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            let Some(age) = meta
                .modified()
                .ok()
                .and_then(|mtime| now.duration_since(mtime).ok())
            else {
                continue;
            };
            if age >= min_age {
                size = size.saturating_add(meta.len());
                paths.push(path);
            }
        }
    }
    (paths, size, complete)
}

fn add_age_sample(preview: &mut AgePreview, size: u64, age: Duration) {
    preview.total_files += 1;
    preview.total_size = preview.total_size.saturating_add(size);
    for (days, files, bytes) in [
        (
            7,
            &mut preview.older_than_7_days_files,
            &mut preview.older_than_7_days_size,
        ),
        (
            30,
            &mut preview.older_than_30_days_files,
            &mut preview.older_than_30_days_size,
        ),
        (
            90,
            &mut preview.older_than_90_days_files,
            &mut preview.older_than_90_days_size,
        ),
    ] {
        if age >= Duration::from_secs(days * 24 * 3600) {
            *files += 1;
            *bytes = bytes.saturating_add(size);
        }
    }
}

fn collect_files_with_age(
    bases: &[PathBuf],
    cancel: &AtomicBool,
    min_age_days: Option<u32>,
    max_files: usize,
) -> (Vec<PathBuf>, u64, AgePreview, bool) {
    let mut eligible = Vec::new();
    let mut eligible_size = 0_u64;
    let mut preview = AgePreview::default();
    let mut stack = bases.to_vec();
    let now = SystemTime::now();
    let min_age = min_age_days.map(|days| Duration::from_secs(days as u64 * 24 * 3600));
    let mut visited = 0_usize;
    let mut complete = true;
    while let Some(path) = stack.pop() {
        if cancel.load(Ordering::Relaxed) || visited >= max_files {
            complete = false;
            break;
        }
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            let Ok(entries) = std::fs::read_dir(path) else {
                continue;
            };
            stack.extend(entries.flatten().map(|entry| entry.path()));
            continue;
        }
        visited += 1;
        let age = meta
            .modified()
            .ok()
            .and_then(|mtime| now.duration_since(mtime).ok())
            .unwrap_or_default();
        add_age_sample(&mut preview, meta.len(), age);
        if min_age.map_or(true, |threshold| age >= threshold) {
            eligible_size = eligible_size.saturating_add(meta.len());
            eligible.push(path);
        }
    }
    (eligible, eligible_size, preview, complete)
}

pub fn scan_junk(cancel: &AtomicBool) -> Vec<JunkHit> {
    let mut hits = Vec::new();
    for rule in RULES {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let bases = rule_paths(rule.id);
        if bases.is_empty() {
            continue;
        }
        let mut paths = Vec::new();
        let mut size = 0u64;
        let mut note = String::new();
        let mut quality = EstimateQuality::Complete;
        let mut age_preview = AgePreview::default();

        match rule.id {
            "user_temp" => {
                for b in &bases {
                    let (ps, sz, complete) = collect_old_temp_files(
                        b,
                        cancel,
                        Duration::from_secs(7 * 24 * 3600),
                        200_000,
                    );
                    size = size.saturating_add(sz);
                    paths.extend(ps);
                    if !complete {
                        quality = EstimateQuality::Truncated;
                    }
                }
                note = format!("{} 个超过 7 天的临时文件", paths.len());
                let (_, _, preview, complete) =
                    collect_files_with_age(&bases, cancel, Some(7), 200_000);
                age_preview = preview;
                if !complete {
                    quality = EstimateQuality::Truncated;
                }
            }
            "downloads_large_old" => {
                for b in &bases {
                    let (ps, sz, complete) = collect_large_old_files(b, cancel);
                    size += sz;
                    paths.extend(ps);
                    if !complete {
                        quality = EstimateQuality::Truncated;
                    }
                }
                note = format!("{} 个文件", paths.len());
            }
            "thumbcache" => {
                for b in &bases {
                    let (ps, sz) = collect_thumbcache_files(b);
                    size += sz;
                    paths.extend(ps);
                }
                note = format!("{} 个数据库", paths.len());
            }
            "edge_cookies" | "chrome_cookies" | "edge_history" | "chrome_history" => {
                for b in &bases {
                    if let Ok(meta) = std::fs::metadata(b) {
                        size += meta.len();
                        paths.push(b.clone());
                    }
                }
                note = "隐私数据·默认不勾选".into();
            }
            "wechat_cache" | "qq_cache" | "wps_cache" | "vscode_cache" | "jetbrains_cache"
            | "steam_cache" => {
                let (ps, sz, preview, complete) =
                    collect_files_with_age(&bases, cancel, rule.min_age_days, 400_000);
                paths = ps;
                size = sz;
                age_preview = preview;
                if !complete {
                    quality = EstimateQuality::Truncated;
                }
                note = format!(
                    "{} 个达到 {} 天的文件（全部默认不勾选）",
                    paths.len(),
                    rule.min_age_days.unwrap_or(0)
                );
            }
            _ => {
                for b in &bases {
                    let (sz, files, complete) = quick_dir_size_with_status(b, cancel, 400_000);
                    size += sz;
                    paths.push(b.clone());
                    if !complete {
                        quality = EstimateQuality::Truncated;
                    }
                    if !note.is_empty() {
                        note.push_str(" · ");
                    }
                    note.push_str(&format!("{} 文件约 {}", files, format_bytes(sz)));
                }
                let (_, _, preview, complete) =
                    collect_files_with_age(&bases, cancel, None, 400_000);
                age_preview = preview;
                if !complete {
                    quality = EstimateQuality::Truncated;
                }
            }
        }

        if age_preview.total_files == 0 {
            let (_, _, preview, complete) = collect_files_with_age(&bases, cancel, None, 400_000);
            age_preview = preview;
            if !complete {
                quality = EstimateQuality::Truncated;
            }
        }
        if size == 0 && paths.is_empty() {
            continue;
        }
        let sensitive = rule.sensitive || paths.iter().any(|p| is_sensitive_path(p));
        let selected = rule.default_selected && !sensitive && size > 0;
        hits.push(JunkHit {
            rule_id: rule.id.to_string(),
            title: rule.title.to_string(),
            detail: rule.detail.to_string(),
            paths,
            size,
            sensitive,
            selected,
            note,
            quality,
            source: Some(rule.source),
            rebuildability: Some(rule.rebuildability),
            user_content_risk: Some(rule.user_content_risk),
            min_age_days: rule.min_age_days,
            requires_process_exit: rule.requires_process_exit,
            age_preview,
        });
    }
    hits.sort_by(|a, b| b.size.cmp(&a.size));
    hits
}

pub fn junk_selected_paths(hits: &[JunkHit]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for h in hits {
        if !h.selected {
            continue;
        }
        for p in &h.paths {
            let key = p
                .canonicalize()
                .unwrap_or_else(|_| p.clone())
                .to_string_lossy()
                .to_ascii_lowercase();
            if seen.insert(key) {
                out.push(p.clone());
            }
        }
    }
    out
}

/// 仅勾选「安全」项：非敏感且规则默认勾选
pub fn apply_safe_selection(hits: &mut [JunkHit]) {
    for h in hits.iter_mut() {
        h.selected = !h.sensitive && h.size > 0 && is_safe_rule(&h.rule_id);
    }
}

fn is_safe_rule(id: &str) -> bool {
    matches!(
        id,
        "user_temp"
            | "thumbcache"
            | "chrome_cache"
            | "edge_cache"
            | "firefox_cache"
            | "directx_shader"
    )
}

pub fn safe_selected_size(hits: &[JunkHit]) -> u64 {
    hits.iter()
        .filter(|h| h.selected && !h.sensitive && is_safe_rule(&h.rule_id))
        .map(|h| h.size)
        .sum()
}

/// 仅保留安全白名单内、非敏感且已勾选的项（用于安静清理）。
/// 与「一键安全清理」共用 `is_safe_rule`，避免规则默认勾选变化后计划任务误清。
pub fn safe_junk_hits(hits: Vec<JunkHit>) -> Vec<JunkHit> {
    hits.into_iter()
        .filter(|h| !h.sensitive && h.selected && h.size > 0 && is_safe_rule(&h.rule_id))
        .collect()
}

/// 用户排除列表同样约束清理（不只约束扫描）：过滤掉命中排除前缀的路径。
pub fn filter_excluded_paths(paths: Vec<PathBuf>, excludes: &[String]) -> Vec<PathBuf> {
    if excludes.is_empty() {
        return paths;
    }
    let ex: Vec<PathBuf> = excludes.iter().map(PathBuf::from).collect();
    let set = crate::scan::ExcludeSet::new(&ex);
    paths.into_iter().filter(|p| !set.contains(p)).collect()
}

pub fn reclaimable_estimate(junk: &[JunkHit], dup_waste: u64) -> u64 {
    let junk_sz: u64 = junk.iter().filter(|h| h.selected).map(|h| h.size).sum();
    junk_sz.saturating_add(dup_waste)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluded_paths_are_filtered_from_clean() {
        let paths = vec![
            PathBuf::from(r"C:\Users\a\AppData\Local\Temp"),
            PathBuf::from(r"D:\Keep\cache"),
        ];
        let excludes = vec![r"D:\Keep".to_string()];
        let out = filter_excluded_paths(paths, &excludes);
        assert_eq!(out, vec![PathBuf::from(r"C:\Users\a\AppData\Local\Temp")]);
    }

    #[test]
    fn quiet_clean_only_takes_safe_rules() {
        let mk = |id: &str, sensitive: bool, selected: bool| JunkHit {
            rule_id: id.to_string(),
            selected,
            sensitive,
            size: 100,
            ..Default::default()
        };
        let hits = vec![
            mk("user_temp", false, true),
            // 非安全白名单规则即使被勾选也不进入安静清理
            mk("windows_old", false, true),
            mk("chrome_cache", true, true),
            mk("edge_cache", false, false),
        ];
        let safe = safe_junk_hits(hits);
        assert_eq!(safe.len(), 1);
        assert_eq!(safe[0].rule_id, "user_temp");
    }

    #[test]
    fn temp_collection_skips_recent_files() {
        let dir = tempfile::tempdir().unwrap();
        let recent = dir.path().join("recent.tmp");
        std::fs::write(&recent, b"recent").unwrap();
        let cancel = AtomicBool::new(false);
        let (paths, _, _) =
            collect_old_temp_files(dir.path(), &cancel, Duration::from_secs(24 * 3600), 100);
        assert!(!paths.contains(&recent));
    }

    #[test]
    fn precision_rules_are_builtin_and_never_preselected() {
        let ids = [
            "wechat_cache",
            "qq_cache",
            "wps_cache",
            "vscode_cache",
            "jetbrains_cache",
            "steam_cache",
        ];
        for id in ids {
            let rule = builtin_junk_rules()
                .iter()
                .find(|rule| rule.id == id)
                .unwrap();
            assert!(!rule.default_selected);
            assert_eq!(rule.min_age_days, Some(7));
            assert!(rule.requires_process_exit);
            assert_ne!(rule.rebuildability, Rebuildability::NotRebuildable);
        }
    }

    #[test]
    fn age_preview_uses_cumulative_thresholds() {
        let mut preview = AgePreview::default();
        add_age_sample(&mut preview, 10, Duration::from_secs(8 * 24 * 3600));
        add_age_sample(&mut preview, 20, Duration::from_secs(40 * 24 * 3600));
        add_age_sample(&mut preview, 30, Duration::from_secs(100 * 24 * 3600));
        assert_eq!(preview.total_size, 60);
        assert_eq!(preview.older_than_7_days_size, 60);
        assert_eq!(preview.older_than_30_days_size, 50);
        assert_eq!(preview.older_than_90_days_size, 30);
    }

    #[test]
    fn product_cache_paths_only_use_fixed_child_names() {
        let mut paths = Vec::new();
        push_product_caches(
            &mut paths,
            Path::new(r"C:\Users\a\AppData\Local\Tencent"),
            &["QQNT"],
        );
        assert_eq!(
            paths,
            vec![
                PathBuf::from(r"C:\Users\a\AppData\Local\Tencent\QQNT\Cache"),
                PathBuf::from(r"C:\Users\a\AppData\Local\Tencent\QQNT\Code Cache"),
                PathBuf::from(r"C:\Users\a\AppData\Local\Tencent\QQNT\GPUCache"),
            ]
        );
    }
}
