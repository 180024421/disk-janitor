//! 扫描结果导出与对比快照

use crate::model::{format_bytes, FsEntry, ScanIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSnapshot {
    pub root: String,
    pub saved_at: String,
    pub total_size: u64,
    pub file_count: u64,
    pub dir_count: u64,
    /// 规范化路径 -> 大小（仅保留较大项，控制体积）
    pub sizes: HashMap<String, u64>,
}

#[derive(Debug, Clone)]
pub struct DiffItem {
    pub path: String,
    pub old_size: u64,
    pub new_size: u64,
    pub delta: i64,
}

pub fn snapshot_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("disk-janitor").join("last-scan.json")
}

pub fn save_snapshot(index: &ScanIndex) -> Result<PathBuf, String> {
    let root_size = index.get(&index.root).map(|e| e.size).unwrap_or(0);
    let mut sizes = HashMap::new();
    let mut entries: Vec<&FsEntry> = index.entries.values().collect();
    entries.sort_by(|a, b| b.size.cmp(&a.size));
    for e in entries.into_iter().take(8_000) {
        sizes.insert(
            e.path.to_string_lossy().to_ascii_lowercase(),
            e.size,
        );
    }
    let snap = ScanSnapshot {
        root: index.root.display().to_string(),
        saved_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        total_size: root_size,
        file_count: index.file_count,
        dir_count: index.dir_count,
        sizes,
    };
    let p = snapshot_path();
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(&snap).map_err(|e| e.to_string())?;
    fs::write(&p, s).map_err(|e| e.to_string())?;
    Ok(p)
}

pub fn load_snapshot() -> Result<ScanSnapshot, String> {
    let p = snapshot_path();
    let s = fs::read_to_string(&p).map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

pub fn compare_with_snapshot(index: &ScanIndex, old: &ScanSnapshot) -> Vec<DiffItem> {
    let mut diffs = Vec::new();
    let mut seen = HashMap::<String, ()>::new();
    for e in index.entries.values() {
        let key = e.path.to_string_lossy().to_ascii_lowercase();
        seen.insert(key.clone(), ());
        let old_sz = old.sizes.get(&key).copied().unwrap_or(0);
        let delta = e.size as i64 - old_sz as i64;
        if delta.abs() < 1024 * 1024 {
            continue;
        }
        diffs.push(DiffItem {
            path: e.path.display().to_string(),
            old_size: old_sz,
            new_size: e.size,
            delta,
        });
    }
    for (k, old_sz) in &old.sizes {
        if *old_sz < 1024 * 1024 || seen.contains_key(k) {
            continue;
        }
        diffs.push(DiffItem {
            path: k.clone(),
            old_size: *old_sz,
            new_size: 0,
            delta: -(*old_sz as i64),
        });
    }
    diffs.sort_by(|a, b| b.delta.abs().cmp(&a.delta.abs()));
    diffs.truncate(80);
    diffs
}

pub fn export_csv(index: &ScanIndex, path: &Path) -> Result<(), String> {
    let mut lines = vec!["path,is_dir,size,size_human,mtime".to_string()];
    let mut entries: Vec<&FsEntry> = index.entries.values().collect();
    entries.sort_by(|a, b| b.size.cmp(&a.size));
    for e in entries.into_iter().take(50_000) {
        let mtime = crate::model::format_mtime(e.mtime);
        let p = e.path.display().to_string().replace('"', "\"\"");
        lines.push(format!(
            "\"{}\",{},{},{},{}",
            p,
            e.is_dir,
            e.size,
            format_bytes(e.size),
            mtime
        ));
    }
    fs::write(path, lines.join("\r\n")).map_err(|e| e.to_string())
}

pub fn export_json(index: &ScanIndex, path: &Path) -> Result<(), String> {
    #[derive(Serialize)]
    struct Row<'a> {
        path: String,
        name: &'a str,
        is_dir: bool,
        size: u64,
        size_human: String,
    }
    let mut rows: Vec<Row> = index
        .entries
        .values()
        .map(|e| Row {
            path: e.path.display().to_string(),
            name: &e.name,
            is_dir: e.is_dir,
            size: e.size,
            size_human: format_bytes(e.size),
        })
        .collect();
    rows.sort_by(|a, b| b.size.cmp(&a.size));
    rows.truncate(50_000);
    let s = serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?;
    fs::write(path, s).map_err(|e| e.to_string())
}

pub fn default_export_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Desktop")
}
