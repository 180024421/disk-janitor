//! 扫描结果导出与对比快照

use crate::model::{format_bytes, FsEntry, ScanIndex};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "diagnostics.rs"]
pub mod diagnostics;
pub(crate) use crate::persistence::atomic_write;

const SNAPSHOT_LIMIT: usize = 30;
const SNAPSHOT_ENTRY_LIMIT: usize = 8_000;
const DIFF_MIN_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSnapshot {
    pub root: String,
    pub saved_at: String,
    pub total_size: u64,
    pub file_count: u64,
    pub dir_count: u64,
    /// 规范化路径 -> 大小（仅保留较大项，控制体积）
    pub sizes: HashMap<String, u64>,
    /// sizes 中哪些路径是目录；旧快照缺少此字段时保持兼容。
    #[serde(default)]
    pub directories: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffItem {
    pub path: String,
    pub old_size: u64,
    pub new_size: u64,
    pub delta: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrowthItem {
    pub path: String,
    pub old_size: u64,
    pub new_size: u64,
    pub growth: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewLargeFile {
    pub path: String,
    pub size: u64,
}

pub fn snapshot_path() -> PathBuf {
    snapshot_store_dir().join("last-scan.json")
}

fn snapshot_store_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("disk-janitor")
}

fn normalized_path(path: &Path) -> Result<String, String> {
    let absolute = fs::canonicalize(path)
        .or_else(|_| {
            if path.is_absolute() {
                Ok(path.to_path_buf())
            } else {
                std::env::current_dir().map(|cwd| cwd.join(path))
            }
        })
        .map_err(|e| format!("无法规范化路径 {}: {e}", path.display()))?;
    let mut value = ScanIndex::key(&absolute);
    if cfg!(windows) {
        value = value.replace('/', "\\").to_ascii_lowercase();
        if let Some(rest) = value.strip_prefix(r"\\?\unc\") {
            value = format!(r"\\{rest}");
        } else if let Some(rest) = value.strip_prefix(r"\\?\") {
            value = rest.to_string();
        }
    }
    Ok(value)
}

fn normalized_root(root: &Path) -> Result<String, String> {
    normalized_path(root)
}

fn root_bucket(root: &str) -> String {
    let digest = Sha256::digest(root.as_bytes());
    hex::encode(&digest[..16])
}

fn snapshot_dir_for(base: &Path, normalized_root: &str) -> PathBuf {
    base.join("snapshots").join(root_bucket(normalized_root))
}

fn now_id() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .map_err(|e| format!("系统时间早于 UNIX_EPOCH: {e}"))
}

fn make_snapshot(index: &ScanIndex) -> Result<ScanSnapshot, String> {
    let root_size = index.get(&index.root).map(|e| e.size).unwrap_or(0);
    let mut sizes = HashMap::new();
    let mut directories = HashSet::new();
    let mut entries: Vec<&FsEntry> = index.entries.values().collect();
    entries.sort_by(|a, b| b.size.cmp(&a.size));
    for e in entries.into_iter().take(SNAPSHOT_ENTRY_LIMIT) {
        let key = normalized_path(&e.path)?;
        if e.is_dir {
            directories.insert(key.clone());
        }
        sizes.insert(key, e.size);
    }
    Ok(ScanSnapshot {
        root: normalized_root(&index.root)?,
        saved_at: chrono::Local::now().to_rfc3339(),
        total_size: root_size,
        file_count: index.file_count,
        dir_count: index.dir_count,
        sizes,
        directories,
    })
}

fn write_snapshot(path: &Path, snapshot: &ScanSnapshot) -> Result<(), String> {
    crate::persistence::save_json(path, snapshot, true)
}

fn prune_snapshot_dir(dir: &Path) -> Result<(), String> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("读取快照目录 {} 失败: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .collect();
    files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    for stale in files.into_iter().skip(SNAPSHOT_LIMIT) {
        fs::remove_file(&stale)
            .map_err(|e| format!("删除过期快照 {} 失败: {e}", stale.display()))?;
    }
    Ok(())
}

fn save_snapshot_in(index: &ScanIndex, base: &Path) -> Result<PathBuf, String> {
    let snapshot = make_snapshot(index)?;
    let dir = snapshot_dir_for(base, &snapshot.root);
    let path = dir.join(format!("{:032}.json", now_id()?));
    write_snapshot(&path, &snapshot)?;
    prune_snapshot_dir(&dir)?;
    // 兼容旧 load_snapshot：它只是最新快照指针；历史数据始终按根隔离。
    write_snapshot(&base.join("last-scan.json"), &snapshot)?;
    Ok(path)
}

/// 保存到规范化扫描根专属目录，并保留该根最近 30 次快照。
pub fn save_snapshot(index: &ScanIndex) -> Result<PathBuf, String> {
    save_snapshot_in(index, &snapshot_store_dir())
}

fn read_snapshot(path: &Path) -> Result<ScanSnapshot, String> {
    crate::persistence::load_json(path)
        .map_err(|e| format!("解析快照 {} 失败: {e}", path.display()))
}

/// 兼容旧 API：读取最后一次保存的快照。比较时仍会严格校验扫描根。
pub fn load_snapshot() -> Result<ScanSnapshot, String> {
    read_snapshot(&snapshot_path())
}

fn load_recent_snapshots_in(
    root: &Path,
    count: usize,
    base: &Path,
) -> Result<Vec<ScanSnapshot>, String> {
    let normalized = normalized_root(root)?;
    let dir = snapshot_dir_for(base, &normalized);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| format!("读取快照目录 {} 失败: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .collect();
    files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    files
        .into_iter()
        .take(count.min(SNAPSHOT_LIMIT))
        .map(|path| {
            let snapshot = read_snapshot(&path)?;
            validate_roots(&normalized, &snapshot.root)?;
            Ok(snapshot)
        })
        .collect()
}

/// 按扫描根读取最近快照；count 最大按 30 处理。
pub fn load_recent_snapshots(root: &Path, count: usize) -> Result<Vec<ScanSnapshot>, String> {
    load_recent_snapshots_in(root, count, &snapshot_store_dir())
}

pub fn load_last_7_snapshots(root: &Path) -> Result<Vec<ScanSnapshot>, String> {
    load_recent_snapshots(root, 7)
}

pub fn load_last_30_snapshots(root: &Path) -> Result<Vec<ScanSnapshot>, String> {
    load_recent_snapshots(root, 30)
}

fn validate_roots(current_root: &str, snapshot_root: &str) -> Result<(), String> {
    let old = normalized_root(Path::new(snapshot_root))?;
    if current_root == old {
        Ok(())
    } else {
        Err(format!(
            "快照扫描根不一致：当前为“{current_root}”，快照为“{old}”"
        ))
    }
}

/// 严格比较 API：扫描根不一致时拒绝比较并返回明确错误。
pub fn compare_with_snapshot_checked(
    index: &ScanIndex,
    old: &ScanSnapshot,
) -> Result<Vec<DiffItem>, String> {
    let current_root = normalized_root(&index.root)?;
    validate_roots(&current_root, &old.root)?;
    let mut diffs = Vec::new();
    let mut seen = HashSet::new();
    for e in index.entries.values() {
        let key = normalized_path(&e.path)?;
        seen.insert(key.clone());
        let old_sz = old.sizes.get(&key).copied().unwrap_or(0);
        let delta = signed_delta(e.size, old_sz);
        if delta.unsigned_abs() < DIFF_MIN_BYTES {
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
        if *old_sz < DIFF_MIN_BYTES || seen.contains(k) {
            continue;
        }
        diffs.push(DiffItem {
            path: k.clone(),
            old_size: *old_sz,
            new_size: 0,
            delta: signed_delta(0, *old_sz),
        });
    }
    diffs.sort_by(|a, b| b.delta.unsigned_abs().cmp(&a.delta.unsigned_abs()));
    diffs.truncate(80);
    Ok(diffs)
}

/// 旧调用兼容层。新集成应使用 compare_with_snapshot_checked 获取根不一致错误。
pub fn compare_with_snapshot(index: &ScanIndex, old: &ScanSnapshot) -> Vec<DiffItem> {
    compare_with_snapshot_checked(index, old).unwrap_or_default()
}

fn signed_delta(new_size: u64, old_size: u64) -> i64 {
    let delta = new_size as i128 - old_size as i128;
    delta.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

/// 返回相比指定快照增长最快的目录。
pub fn fastest_growing_directories(
    index: &ScanIndex,
    old: &ScanSnapshot,
    limit: usize,
) -> Result<Vec<GrowthItem>, String> {
    let current_root = normalized_root(&index.root)?;
    validate_roots(&current_root, &old.root)?;
    let mut items = Vec::new();
    for entry in index.entries.values().filter(|entry| entry.is_dir) {
        let key = normalized_path(&entry.path)?;
        let old_size = old.sizes.get(&key).copied().unwrap_or(0);
        if entry.size > old_size {
            items.push(GrowthItem {
                path: entry.path.display().to_string(),
                old_size,
                new_size: entry.size,
                growth: entry.size - old_size,
            });
        }
    }
    items.sort_by(|a, b| b.growth.cmp(&a.growth));
    items.truncate(limit);
    Ok(items)
}

/// 返回快照后新增且达到阈值的大文件。
pub fn newly_added_large_files(
    index: &ScanIndex,
    old: &ScanSnapshot,
    min_size: u64,
    limit: usize,
) -> Result<Vec<NewLargeFile>, String> {
    let current_root = normalized_root(&index.root)?;
    validate_roots(&current_root, &old.root)?;
    let mut items = Vec::new();
    for entry in index
        .entries
        .values()
        .filter(|entry| !entry.is_dir && entry.size >= min_size)
    {
        let key = normalized_path(&entry.path)?;
        if !old.sizes.contains_key(&key) {
            items.push(NewLargeFile {
                path: entry.path.display().to_string(),
                size: entry.size,
            });
        }
    }
    items.sort_by(|a, b| b.size.cmp(&a.size));
    items.truncate(limit);
    Ok(items)
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
    atomic_write(path, lines.join("\r\n").as_bytes())
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
    let bytes = serde_json::to_vec_pretty(&rows).map_err(|e| e.to_string())?;
    atomic_write(path, &bytes)
}

pub fn default_export_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Desktop")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn index(root: &Path, sequence: u64) -> ScanIndex {
        let dir = root.join("cache");
        let file = dir.join(format!("new-{sequence}.bin"));
        let mut entries = HashMap::new();
        entries.insert(
            ScanIndex::key(root),
            FsEntry {
                path: root.to_path_buf(),
                name: "root".into(),
                is_dir: true,
                size: 30_000_000 + sequence,
                ..FsEntry::default()
            },
        );
        entries.insert(
            ScanIndex::key(&dir),
            FsEntry {
                path: dir,
                name: "cache".into(),
                is_dir: true,
                size: 20_000_000 + sequence,
                ..FsEntry::default()
            },
        );
        entries.insert(
            ScanIndex::key(&file),
            FsEntry {
                path: file,
                name: format!("new-{sequence}.bin"),
                is_dir: false,
                size: 10_000_000 + sequence,
                ..FsEntry::default()
            },
        );
        ScanIndex {
            root: root.to_path_buf(),
            entries,
            file_count: 1,
            dir_count: 2,
            ..ScanIndex::default()
        }
    }

    #[test]
    fn histories_are_isolated_by_normalized_root_and_pruned() {
        let store = tempdir().unwrap();
        let roots = tempdir().unwrap();
        let root_a = roots.path().join("a");
        let root_b = roots.path().join("b");
        fs::create_dir_all(&root_a).unwrap();
        fs::create_dir_all(&root_b).unwrap();
        for sequence in 0..35 {
            save_snapshot_in(&index(&root_a, sequence), store.path()).unwrap();
        }
        save_snapshot_in(&index(&root_b, 99), store.path()).unwrap();

        let a = load_recent_snapshots_in(&root_a, 30, store.path()).unwrap();
        let b = load_recent_snapshots_in(&root_b, 30, store.path()).unwrap();
        assert_eq!(a.len(), 30);
        assert_eq!(b.len(), 1);
        assert_ne!(a[0].root, b[0].root);
    }

    #[test]
    fn checked_comparison_rejects_a_different_root() {
        let roots = tempdir().unwrap();
        let root_a = roots.path().join("a");
        let root_b = roots.path().join("b");
        fs::create_dir_all(&root_a).unwrap();
        fs::create_dir_all(&root_b).unwrap();
        let snapshot = make_snapshot(&index(&root_a, 1)).unwrap();
        let error = compare_with_snapshot_checked(&index(&root_b, 2), &snapshot).unwrap_err();
        assert!(error.contains("快照扫描根不一致"));
    }

    #[test]
    fn insights_report_growth_and_new_large_files() {
        let root = tempdir().unwrap();
        let old_index = index(root.path(), 1);
        let snapshot = make_snapshot(&old_index).unwrap();
        let mut current = index(root.path(), 2);
        let extra = root.path().join("large.iso");
        current.entries.insert(
            ScanIndex::key(&extra),
            FsEntry {
                path: extra.clone(),
                name: "large.iso".into(),
                is_dir: false,
                size: 50_000_000,
                ..FsEntry::default()
            },
        );
        current
            .entries
            .get_mut(&ScanIndex::key(&root.path().join("cache")))
            .unwrap()
            .size += 40_000_000;

        let growth = fastest_growing_directories(&current, &snapshot, 7).unwrap();
        let files = newly_added_large_files(&current, &snapshot, 20_000_000, 7).unwrap();
        assert_eq!(growth[0].growth, 40_000_001);
        assert_eq!(files[0].path, extra.display().to_string());
    }

    #[test]
    fn atomic_write_replaces_without_leaving_temp_files() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("value.json");
        atomic_write(&path, b"one").unwrap();
        atomic_write(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
