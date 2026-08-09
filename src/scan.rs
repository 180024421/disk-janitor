//! 并行扫盘引擎（支持增量快照）

use crate::model::{FsEntry, ScanIndex};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ScanProgress {
    pub visited: u64,
    pub skipped: u64,
    pub bytes_seen: u64,
    pub current: String,
    pub elapsed: Duration,
    pub done: bool,
    pub cancelled: bool,
    pub error: Option<String>,
}

#[derive(Clone)]
struct Raw {
    path: PathBuf,
    is_dir: bool,
    file_size: u64,
    mtime: Option<SystemTime>,
}

pub enum ScanEvent {
    Progress(ScanProgress),
    /// 边扫边更新的不完整索引
    Partial(ScanIndex),
    Done(ScanIndex),
}

/// 扫描 `root`。通过 `on_event` 推送进度与增量索引。
pub fn scan_path<F>(root: PathBuf, cancel: Arc<AtomicBool>, mut on_event: F) -> ScanIndex
where
    F: FnMut(ScanEvent),
{
    let started = Instant::now();
    let visited = Arc::new(AtomicU64::new(0));
    let skipped = Arc::new(AtomicU64::new(0));
    let bytes_seen = Arc::new(AtomicU64::new(0));

    let mut index = ScanIndex {
        root: root.clone(),
        ..Default::default()
    };

    if !root.exists() {
        let msg = format!("路径不存在: {}", root.display());
        index.errors.push(msg.clone());
        on_event(ScanEvent::Progress(ScanProgress {
            visited: 0,
            skipped: 0,
            bytes_seen: 0,
            current: String::new(),
            elapsed: started.elapsed(),
            done: true,
            cancelled: false,
            error: Some(msg),
        }));
        on_event(ScanEvent::Done(index.clone()));
        return index;
    }

    let mut raws: Vec<Raw> = Vec::new();
    let mut stack = vec![root.clone()];
    let mut last_ui = Instant::now();
    let mut last_partial = Instant::now();

    // 先塞入根，并快速列出第一层，立刻给 UI
    if let Ok(meta) = std::fs::metadata(&root) {
        raws.push(Raw {
            path: root.clone(),
            is_dir: meta.is_dir(),
            file_size: if meta.is_dir() { 0 } else { meta.len() },
            mtime: meta.modified().ok(),
        });
    }
    if let Ok(rd) = std::fs::read_dir(&root) {
        for ent in rd.flatten() {
            let path = ent.path();
            if let Ok(meta) = std::fs::symlink_metadata(&path) {
                let ft = meta.file_type();
                if ft.is_symlink() {
                    continue;
                }
                let is_dir = ft.is_dir();
                let file_size = if is_dir { 0 } else { meta.len() };
                if is_dir {
                    stack.push(path.clone());
                } else {
                    bytes_seen.fetch_add(file_size, Ordering::Relaxed);
                }
                visited.fetch_add(1, Ordering::Relaxed);
                raws.push(Raw {
                    path,
                    is_dir,
                    file_size,
                    mtime: meta.modified().ok(),
                });
            }
        }
        // 根已展开，从 stack 去掉 root 本身（若存在）
        stack.retain(|p| p != &root);
        let snap = finish_counts(build_index(index.clone(), &raws, &root, 0, true));
        on_event(ScanEvent::Partial(snap));
    }

    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            let skipped_n = skipped.load(Ordering::Relaxed);
            let done = finish_counts(build_index(index, &raws, &root, skipped_n, false));
            on_event(ScanEvent::Progress(ScanProgress {
                visited: visited.load(Ordering::Relaxed),
                skipped: skipped_n,
                bytes_seen: bytes_seen.load(Ordering::Relaxed),
                current: dir.display().to_string(),
                elapsed: started.elapsed(),
                done: true,
                cancelled: true,
                error: None,
            }));
            on_event(ScanEvent::Done(done.clone()));
            return done;
        }

        let rd = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => {
                skipped.fetch_add(1, Ordering::Relaxed);
                if index.errors.len() < 40 {
                    index.errors.push(format!("{}: {}", dir.display(), e));
                }
                continue;
            }
        };

        let children: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        let chunk: Vec<Raw> = children
            .par_iter()
            .filter_map(|path| {
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                let meta = match std::fs::symlink_metadata(path) {
                    Ok(m) => m,
                    Err(_) => {
                        skipped.fetch_add(1, Ordering::Relaxed);
                        return None;
                    }
                };
                let ft = meta.file_type();
                if ft.is_symlink() {
                    visited.fetch_add(1, Ordering::Relaxed);
                    return Some(Raw {
                        path: path.clone(),
                        is_dir: false,
                        file_size: meta.len(),
                        mtime: meta.modified().ok(),
                    });
                }
                // 跳过会拖死扫描的系统/缓存目录（仍计 skipped，不进栈）
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if should_skip_dir(name) {
                    skipped.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
                // 跳过联接/重解析点（OneDrive 等），避免扫用户目录时卡死
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
                    if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                        skipped.fetch_add(1, Ordering::Relaxed);
                        return None;
                    }
                }
                let is_dir = ft.is_dir();
                let file_size = if is_dir { 0 } else { meta.len() };
                if !is_dir {
                    bytes_seen.fetch_add(file_size, Ordering::Relaxed);
                }
                visited.fetch_add(1, Ordering::Relaxed);
                Some(Raw {
                    path: path.clone(),
                    is_dir,
                    file_size,
                    mtime: meta.modified().ok(),
                })
            })
            .collect();

        for r in &chunk {
            if r.is_dir {
                stack.push(r.path.clone());
            }
        }
        raws.extend(chunk);

        if last_ui.elapsed() >= Duration::from_millis(200) {
            last_ui = Instant::now();
            on_event(ScanEvent::Progress(ScanProgress {
                visited: visited.load(Ordering::Relaxed),
                skipped: skipped.load(Ordering::Relaxed),
                bytes_seen: bytes_seen.load(Ordering::Relaxed),
                current: dir.display().to_string(),
                elapsed: started.elapsed(),
                done: false,
                cancelled: false,
                error: None,
            }));
        }

        // 增量索引：首层已推过；之后最多每 3s 一次，且条目过大时停推，避免 UI 卡死
        let n = raws.len();
        let interval = if n > 80_000 {
            Duration::from_secs(10)
        } else if n > 20_000 {
            Duration::from_secs(5)
        } else {
            Duration::from_secs(3)
        };
        if n <= 120_000 && last_partial.elapsed() >= interval {
            last_partial = Instant::now();
            let skipped_n = skipped.load(Ordering::Relaxed);
            let snap = finish_counts(build_index(
                ScanIndex {
                    root: root.clone(),
                    errors: index.errors.clone(),
                    ..Default::default()
                },
                &raws,
                &root,
                skipped_n,
                true,
            ));
            on_event(ScanEvent::Partial(snap));
        }
    }

    let skipped_n = skipped.load(Ordering::Relaxed);
    let done = finish_counts(build_index(index, &raws, &root, skipped_n, false));
    on_event(ScanEvent::Progress(ScanProgress {
        visited: visited.load(Ordering::Relaxed),
        skipped: skipped_n,
        bytes_seen: bytes_seen.load(Ordering::Relaxed),
        current: root.display().to_string(),
        elapsed: started.elapsed(),
        done: true,
        cancelled: cancel.load(Ordering::Relaxed),
        error: None,
    }));
    on_event(ScanEvent::Done(done.clone()));
    done
}

fn finish_counts(mut index: ScanIndex) -> ScanIndex {
    index.rebuild_children();
    index.file_count = index.entries.values().filter(|e| !e.is_dir).count() as u64;
    index.dir_count = index.entries.values().filter(|e| e.is_dir).count() as u64;
    index
}

fn should_skip_dir(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "$recycle.bin"
        || lower == "system volume information"
        || lower == "winsxs"
        || lower == "installer"
        || lower == "servicing"
        || lower == "softwaredistribution"
        || lower == "package cache"
        || lower == "node_modules"
        || lower == ".git"
        || lower == ".svn"
        || lower == "csc"
        || lower == "onedrive"
        || lower == "onedrivetemp"
        || lower.starts_with("onedrive")
}

fn build_index(
    mut index: ScanIndex,
    raws: &[Raw],
    root: &Path,
    skipped: u64,
    partial: bool,
) -> ScanIndex {
    index.skipped = skipped;
    index.partial = partial;
    let mut entries: HashMap<String, FsEntry> = HashMap::new();
    let mut dir_sizes: HashMap<String, u64> = HashMap::new();

    for r in raws {
        let key = ScanIndex::key(&r.path);
        let name = r
            .path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| r.path.display().to_string());
        entries.insert(
            key,
            FsEntry {
                path: r.path.clone(),
                name,
                is_dir: r.is_dir,
                size: r.file_size,
                mtime: r.mtime,
            },
        );
        if !r.is_dir && r.file_size > 0 {
            for anc in r.path.ancestors().skip(1) {
                if anc.as_os_str().is_empty() {
                    break;
                }
                if !anc.starts_with(root) {
                    continue;
                }
                let k = ScanIndex::key(anc);
                *dir_sizes.entry(k).or_insert(0) += r.file_size;
            }
        }
    }

    for (k, e) in entries.iter_mut() {
        if e.is_dir {
            e.size = *dir_sizes.get(k).unwrap_or(&0);
        }
    }

    index.entries = entries;
    index
}

/// 快速估算目录占用（垃圾规则用，可取消）
pub fn quick_dir_size(path: &Path, cancel: &AtomicBool, max_files: u64) -> (u64, u64) {
    let mut total = 0u64;
    let mut files = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            if cancel.load(Ordering::Relaxed) || files >= max_files {
                return (total, files);
            }
            let p = ent.path();
            let Ok(meta) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(p);
            } else {
                total += meta.len();
                files += 1;
            }
        }
    }
    (total, files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn aggregates_directory_size() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = a.join("b");
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("f1.txt"), vec![1u8; 1000]).unwrap();
        fs::write(b.join("f2.txt"), vec![1u8; 500]).unwrap();

        let cancel = Arc::new(AtomicBool::new(false));
        let idx = scan_path(dir.path().to_path_buf(), cancel, |_| {});
        let root = idx.get(dir.path()).expect("root");
        assert!(root.is_dir);
        assert_eq!(root.size, 1500);
        assert_eq!(idx.get(&a).expect("a").size, 1500);
        assert_eq!(idx.get(&b).expect("b").size, 500);
        assert!(!idx.partial);
    }

    #[test]
    fn sorts_children_and_filter_keys() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("big.bin"), vec![1u8; 3000]).unwrap();
        fs::write(dir.path().join("small.bin"), vec![1u8; 10]).unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        fs::write(dir.path().join("subdir").join("x"), vec![1u8; 100]).unwrap();

        let cancel = Arc::new(AtomicBool::new(false));
        let idx = scan_path(dir.path().to_path_buf(), cancel, |_| {});
        let mut kids = idx.children_of(dir.path());
        crate::model::sort_entries(
            &mut kids,
            crate::model::SortKey::Size,
            crate::model::SortDir::Desc,
        );
        assert!(kids[0].is_dir);
        let files: Vec<_> = kids.iter().filter(|e| !e.is_dir).collect();
        assert!(files[0].size >= files[1].size);
    }

    #[test]
    fn remove_cascade_updates_parents() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        fs::create_dir_all(&a).unwrap();
        fs::write(a.join("f1.txt"), vec![1u8; 1000]).unwrap();
        fs::write(dir.path().join("keep.txt"), vec![1u8; 100]).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut idx = scan_path(dir.path().to_path_buf(), cancel, |_| {});
        assert_eq!(idx.get(dir.path()).unwrap().size, 1100);
        idx.remove_cascade(&a);
        assert!(idx.get(&a).is_none());
        assert_eq!(idx.get(dir.path()).unwrap().size, 100);
    }
}
