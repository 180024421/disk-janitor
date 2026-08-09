//! 二期：常见垃圾规则建议

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
}

#[derive(Debug, Clone)]
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
    },
    JunkRule {
        id: "win_temp",
        title: "Windows\\Temp",
        detail: "系统临时目录（可能需管理员）",
        sensitive: true,
    },
    JunkRule {
        id: "prefetch",
        title: "Prefetch",
        detail: "预读取缓存（一般可清，敏感）",
        sensitive: true,
    },
    JunkRule {
        id: "chrome_cache",
        title: "Chrome 缓存",
        detail: "Local\\Google\\Chrome\\User Data\\Default\\Cache",
        sensitive: false,
    },
    JunkRule {
        id: "edge_cache",
        title: "Edge 缓存",
        detail: "Local\\Microsoft\\Edge\\User Data\\Default\\Cache",
        sensitive: false,
    },
    JunkRule {
        id: "downloads_large_old",
        title: "下载目录：大而旧的文件",
        detail: "Downloads 中 >100MB 且超过 90 天",
        sensitive: false,
    },
];

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).map(PathBuf::from)
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
        "chrome_cache" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.push(
                    local
                        .join("Google")
                        .join("Chrome")
                        .join("User Data")
                        .join("Default")
                        .join("Cache"),
                );
            }
        }
        "edge_cache" => {
            if let Some(local) = env_path("LOCALAPPDATA") {
                out.push(
                    local
                        .join("Microsoft")
                        .join("Edge")
                        .join("User Data")
                        .join("Default")
                        .join("Cache"),
                );
            }
        }
        "downloads_large_old" => {
            if let Some(user) = env_path("USERPROFILE") {
                out.push(user.join("Downloads"));
            }
        }
        _ => {}
    }
    out.into_iter().filter(|p| p.exists()).collect()
}

fn collect_large_old_files(dir: &Path, cancel: &AtomicBool) -> (Vec<PathBuf>, u64) {
    let mut paths = Vec::new();
    let mut size = 0u64;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return (paths, size);
    };
    let now = SystemTime::now();
    let max_age = Duration::from_secs(90 * 24 * 3600);
    let min_size = 100u64 * 1024 * 1024;
    for ent in rd.flatten() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let p = ent.path();
        let Ok(meta) = std::fs::metadata(&p) else {
            continue;
        };
        if !meta.is_file() || meta.len() < min_size {
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
    (paths, size)
}

/// 扫描垃圾建议（后台线程调用）
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

        if rule.id == "downloads_large_old" {
            for b in &bases {
                let (ps, sz) = collect_large_old_files(b, cancel);
                size += sz;
                paths.extend(ps);
            }
            note = format!("{} 个文件", paths.len());
        } else {
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

        if size == 0 && paths.is_empty() {
            continue;
        }
        let sensitive = rule.sensitive || paths.iter().any(|p| is_sensitive_path(p));
        hits.push(JunkHit {
            rule_id: rule.id.to_string(),
            title: rule.title.to_string(),
            detail: rule.detail.to_string(),
            paths,
            size,
            sensitive,
            selected: !sensitive && size > 0,
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
