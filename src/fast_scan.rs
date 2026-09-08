//! 极速扫描：优先 NTFS MFT（需管理员），失败回退 turbo walk

use crate::model::{EstimateQuality, FsEntry, ScanIndex};
use crate::scan::{scan_path_ex, ExcludeSet, ScanEvent, ScanOptions, ScanProgress};
use ntfs_reader::file_info::{FileInfo, HashMapCache};
use ntfs_reader::mft::Mft;
use ntfs_reader::volume::Volume;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// 极速 walk（展开 node_modules/.git）
pub fn scan_turbo<F>(
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    excludes: &[PathBuf],
    on_event: F,
) -> ScanIndex
where
    F: FnMut(ScanEvent),
{
    scan_path_ex(
        root,
        cancel,
        on_event,
        ScanOptions {
            excludes: excludes.to_vec(),
            turbo: true,
        },
    )
}

/// 读取整盘 MFT 并构建索引（仅盘符根，如 `C:\`；需管理员）
pub fn scan_mft_drive(
    letter: char,
    cancel: &AtomicBool,
    excludes: &[PathBuf],
    mut on_event: impl FnMut(ScanEvent),
) -> Result<ScanIndex, String> {
    let letter = letter.to_ascii_uppercase();
    if !letter.is_ascii_alphabetic() {
        return Err("无效盘符".into());
    }
    let root = PathBuf::from(format!("{letter}:\\"));
    let vol_path = format!("\\\\.\\{letter}:");
    let started = Instant::now();

    let volume =
        Volume::new(&vol_path).map_err(|e| format!("打开卷失败（通常需要管理员权限）: {e}"))?;
    let mft = Mft::new(volume).map_err(|e| format!("读取 MFT 失败: {e}"))?;

    let exclude_set = ExcludeSet::new(excludes);

    let mut index = ScanIndex {
        root: root.clone(),
        ..Default::default()
    };
    // 根目录条目
    index.entries.insert(
        ScanIndex::key(&root),
        FsEntry {
            path: root.clone(),
            name: format!("{letter}:"),
            is_dir: true,
            size: 0,
            mtime: None,
            count_only: false,
        },
    );

    let mut cache = HashMapCache::default();
    let mut visited = 0u64;
    let mut total_bytes = 0u64;
    let mut last_ui = Instant::now();
    let mut dir_sizes: HashMap<String, u64> = HashMap::new();

    for file in mft.files() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let info = FileInfo::with_cache(&mft, &file, &mut cache);
        if info.path.as_os_str().is_empty() {
            continue;
        }
        // 只要该盘上的路径
        let path = if info.path.has_root() {
            info.path.clone()
        } else {
            root.join(&info.path)
        };
        let path_s = path.to_string_lossy();
        if !path_s
            .to_ascii_lowercase()
            .starts_with(&format!("{letter}:\\").to_ascii_lowercase())
            && path_s.to_ascii_lowercase() != format!("{letter}:").to_ascii_lowercase()
            && path_s.to_ascii_lowercase() != format!("{letter}:\\").to_ascii_lowercase()
        {
            // FileInfo path 有时已是完整路径
            if !path_s
                .chars()
                .next()
                .map(|c| c.eq_ignore_ascii_case(&letter))
                .unwrap_or(false)
            {
                continue;
            }
        }

        // 与普通扫描的 Ignore 目录保持一致（回收站 / 卷影 / 脱机缓存）
        if in_system_ignored_dir(&path) {
            continue;
        }
        if exclude_set.contains(&path) {
            continue;
        }

        visited += 1;
        let key = ScanIndex::key(&path);
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let mtime: Option<SystemTime> = info.modified.and_then(|value| {
            let seconds = value.unix_timestamp();
            if seconds < 0 {
                None
            } else {
                Some(
                    std::time::UNIX_EPOCH
                        + Duration::from_secs(seconds as u64)
                        + Duration::from_nanos(value.nanosecond() as u64),
                )
            }
        });
        let size = if info.is_directory { 0 } else { info.size };

        index.entries.insert(
            key.clone(),
            FsEntry {
                path: path.clone(),
                name,
                is_dir: info.is_directory,
                size,
                mtime,
                count_only: false,
            },
        );

        if !info.is_directory && size > 0 {
            total_bytes += size;
            for anc in path.ancestors().skip(1) {
                if anc.as_os_str().is_empty() {
                    break;
                }
                let ak = ScanIndex::key(anc);
                *dir_sizes.entry(ak).or_insert(0) += size;
            }
        }

        if last_ui.elapsed() >= Duration::from_millis(300) {
            last_ui = Instant::now();
            on_event(ScanEvent::Progress(ScanProgress {
                visited,
                skipped: 0,
                bytes_seen: total_bytes,
                current: path.display().to_string(),
                elapsed: started.elapsed(),
                done: false,
                cancelled: false,
                error: None,
            }));
        }
    }

    for (k, sz) in &dir_sizes {
        if let Some(e) = index.entries.get_mut(k) {
            if e.is_dir {
                e.size = *sz;
            }
        } else {
            // 祖先可能缺失，补目录壳
            let p = PathBuf::from(k);
            let name = p
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| p.display().to_string());
            index.entries.insert(
                k.clone(),
                FsEntry {
                    path: p,
                    name,
                    is_dir: true,
                    size: *sz,
                    mtime: None,
                    count_only: false,
                },
            );
        }
    }

    // 确保根大小
    if let Some(e) = index.entries.get_mut(&ScanIndex::key(&root)) {
        e.size = *dir_sizes.get(&ScanIndex::key(&root)).unwrap_or(&e.size);
    }

    index.partial = cancel.load(Ordering::Relaxed);
    index.quality = if index.partial {
        EstimateQuality::Estimated
    } else {
        EstimateQuality::Complete
    };
    index.rebuild_children();
    index.file_count = index.entries.values().filter(|e| !e.is_dir).count() as u64;
    index.dir_count = index.entries.values().filter(|e| e.is_dir).count() as u64;

    let cancelled = cancel.load(Ordering::Relaxed);
    on_event(ScanEvent::Progress(ScanProgress {
        visited,
        skipped: 0,
        bytes_seen: index
            .entries
            .get(&ScanIndex::key(&root))
            .map(|e| e.size)
            .unwrap_or(0),
        current: root.display().to_string(),
        elapsed: started.elapsed(),
        done: true,
        cancelled,
        error: None,
    }));
    on_event(ScanEvent::Done(index.clone()));
    Ok(index)
}

/// 是否位于普通扫描会直接忽略的系统目录内（`$Recycle.Bin` 等）。
fn in_system_ignored_dir(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s.eq_ignore_ascii_case("$Recycle.Bin")
            || s.eq_ignore_ascii_case("System Volume Information")
            || s.eq_ignore_ascii_case("csc")
    })
}

/// 盘符根且管理员时走 MFT，否则 turbo walk。
pub fn try_fast_scan<F>(
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    excludes: &[PathBuf],
    mut on_event: F,
) -> ScanIndex
where
    F: FnMut(ScanEvent),
{
    let is_drive_root = {
        let s = root.to_string_lossy();
        let b = s.as_bytes();
        (b.len() == 2 && b[1] == b':')
            || (b.len() == 3 && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
    };
    if is_drive_root {
        if let Some(letter) = drive_letter(&root) {
            match scan_mft_drive(letter, cancel.as_ref(), excludes, &mut on_event) {
                Ok(idx) => return idx,
                Err(e) => {
                    // 通知 UI 后回退
                    on_event(ScanEvent::Progress(ScanProgress {
                        visited: 0,
                        skipped: 0,
                        bytes_seen: 0,
                        current: format!("MFT 不可用，回退极速 walk：{e}"),
                        elapsed: Duration::ZERO,
                        done: false,
                        cancelled: false,
                        error: None,
                    }));
                }
            }
        }
    }
    scan_turbo(root, cancel, excludes, on_event)
}

fn drive_letter(root: &Path) -> Option<char> {
    let s = root.to_string_lossy();
    let mut chars = s.chars();
    let c = chars.next()?;
    if c.is_ascii_alphabetic() && chars.next() == Some(':') {
        Some(c.to_ascii_uppercase())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn turbo_expands_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir_all(&nm).unwrap();
        fs::write(nm.join("pkg.js"), vec![1u8; 1500]).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let idx = scan_turbo(dir.path().to_path_buf(), cancel, &[], |_| {});
        let e = idx.get(&nm).expect("node_modules present");
        assert!(!e.count_only, "turbo should fully walk node_modules");
        assert_eq!(e.size, 1500);
    }

    #[test]
    fn system_dirs_are_ignored_in_mft() {
        assert!(in_system_ignored_dir(Path::new(
            r"C:\$Recycle.Bin\S-1-5-21\file"
        )));
        assert!(in_system_ignored_dir(Path::new(
            r"D:\System Volume Information\x"
        )));
        assert!(!in_system_ignored_dir(Path::new(
            r"C:\Users\a\Recycle.Bin.txt"
        )));
        assert!(!in_system_ignored_dir(Path::new(r"C:\Windows\Temp")));
    }

    #[test]
    fn mft_without_admin_errors_or_ok() {
        let cancel = AtomicBool::new(false);
        // 无管理员时通常 Err；有管理员时可能 Ok — 只要不 panic
        let _ = scan_mft_drive('C', &cancel, &[], |_| {});
    }
}
