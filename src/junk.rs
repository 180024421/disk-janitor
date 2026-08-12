//! 常见垃圾规则建议

use crate::model::{format_bytes, is_sensitive_path};
use crate::scan::quick_dir_size;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct JunkRule {
    pub id: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    pub sensitive: bool,
    /// 默认是否勾选
    pub default_selected: bool,
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
}

const RULES: &[JunkRule] = &[
    JunkRule {
        id: "user_temp",
        title: "用户临时文件",
        detail: "%TEMP% / Local\\Temp",
        sensitive: false,
        default_selected: true,
    },
    JunkRule {
        id: "win_temp",
        title: "Windows\\Temp",
        detail: "系统临时目录（可能需管理员）",
        sensitive: true,
        default_selected: false,
    },
    JunkRule {
        id: "prefetch",
        title: "Prefetch",
        detail: "预读取缓存（清理后可能拖慢开机，默认不勾选）",
        sensitive: true,
        default_selected: false,
    },
    JunkRule {
        id: "thumbcache",
        title: "缩略图缓存",
        detail: "Explorer 缩略图数据库",
        sensitive: false,
        default_selected: true,
    },
    JunkRule {
        id: "delivery_opt",
        title: "传递优化缓存",
        detail: "Windows Delivery Optimization",
        sensitive: true,
        default_selected: false,
    },
    JunkRule {
        id: "chrome_cache",
        title: "Chrome 缓存",
        detail: "Cache / Code Cache / GPUCache（Default）",
        sensitive: false,
        default_selected: true,
    },
    JunkRule {
        id: "edge_cache",
        title: "Edge 缓存",
        detail: "Cache / Code Cache / GPUCache（Default）",
        sensitive: false,
        default_selected: true,
    },
    JunkRule {
        id: "firefox_cache",
        title: "Firefox 缓存",
        detail: "Local\\Mozilla\\Firefox\\Profiles\\*\\cache2",
        sensitive: false,
        default_selected: true,
    },
    JunkRule {
        id: "npm_cache",
        title: "npm 缓存",
        detail: "%LOCALAPPDATA%\\npm-cache",
        sensitive: false,
        default_selected: false,
    },
    JunkRule {
        id: "pip_cache",
        title: "pip 缓存",
        detail: "Local\\pip\\Cache",
        sensitive: false,
        default_selected: false,
    },
    JunkRule {
        id: "cargo_cache",
        title: "Cargo 注册表缓存",
        detail: "%USERPROFILE%\\.cargo\\registry\\cache",
        sensitive: false,
        default_selected: false,
    },
    JunkRule {
        id: "downloads_large_old",
        title: "下载目录：大而旧的文件",
        detail: "Downloads 中 >100MB 且超过 90 天（含子文件夹）",
        sensitive: false,
        default_selected: false,
    },
    JunkRule {
        id: "win_update_download",
        title: "Windows Update 下载缓存",
        detail: "SoftwareDistribution\\Download（敏感，默认不勾选）",
        sensitive: true,
        default_selected: false,
    },
    JunkRule {
        id: "chrome_cache_profiles",
        title: "Chrome 其他配置缓存",
        detail: "User Data\\Profile *\\Cache 等（默认不勾选）",
        sensitive: false,
        default_selected: false,
    },
    JunkRule {
        id: "edge_profile_cache",
        title: "Edge 其他配置缓存",
        detail: "User Data\\Profile *\\Cache 等（默认不勾选）",
        sensitive: false,
        default_selected: false,
    },
    JunkRule {
        id: "windows_old",
        title: "Windows.old",
        detail: "系统盘根目录 Windows.old（升级残留，体积大）",
        sensitive: true,
        default_selected: false,
    },
    JunkRule {
        id: "cbs_logs",
        title: "CBS 日志",
        detail: "Windows\\Logs\\CBS",
        sensitive: true,
        default_selected: false,
    },
    JunkRule {
        id: "font_cache",
        title: "字体缓存",
        detail: "Local\\FontCache / Service\\FontCache",
        sensitive: false,
        default_selected: false,
    },
    JunkRule {
        id: "directx_shader",
        title: "DirectX 着色器缓存",
        detail: "D3DSCache / DXCache",
        sensitive: false,
        default_selected: true,
    },
    JunkRule {
        id: "edge_cookies",
        title: "隐私·Edge Cookies（默认不勾选）",
        detail: "User Data\\Default\\Network\\Cookies",
        sensitive: true,
        default_selected: false,
    },
    JunkRule {
        id: "chrome_cookies",
        title: "隐私·Chrome Cookies（默认不勾选）",
        detail: "User Data\\Default\\Network\\Cookies",
        sensitive: true,
        default_selected: false,
    },
    JunkRule {
        id: "edge_history",
        title: "隐私·Edge 历史（默认不勾选）",
        detail: "User Data\\Default\\History",
        sensitive: true,
        default_selected: false,
    },
    JunkRule {
        id: "chrome_history",
        title: "隐私·Chrome 历史（默认不勾选）",
        detail: "User Data\\Default\\History",
        sensitive: true,
        default_selected: false,
    },
];

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
                out.push(
                    local
                        .join("Microsoft")
                        .join("Windows")
                        .join("Explorer"),
                );
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
                out.push(
                    local
                        .join("Microsoft")
                        .join("Windows")
                        .join("DXCache"),
                );
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
        _ => {}
    }
    out.into_iter().filter(|p| p.exists()).collect()
}

fn collect_large_old_files(dir: &Path, cancel: &AtomicBool) -> (Vec<PathBuf>, u64) {
    let mut paths = Vec::new();
    let mut size = 0u64;
    let now = SystemTime::now();
    let max_age = Duration::from_secs(90 * 24 * 3600);
    let min_size = 100u64 * 1024 * 1024;
    let mut stack = vec![dir.to_path_buf()];
    let mut visited = 0u64;
    while let Some(d) = stack.pop() {
        if cancel.load(Ordering::Relaxed) || visited > 20_000 {
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
    (paths, size)
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

        match rule.id {
            "downloads_large_old" => {
                for b in &bases {
                    let (ps, sz) = collect_large_old_files(b, cancel);
                    size += sz;
                    paths.extend(ps);
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
            _ => {
                for b in &bases {
                    let (sz, files) = quick_dir_size(b, cancel, 400_000);
                    size += sz;
                    paths.push(b.clone());
                    if !note.is_empty() {
                        note.push_str(" · ");
                    }
                    note.push_str(&format!("{} 文件约 {}", files, format_bytes(sz)));
                }
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
}
