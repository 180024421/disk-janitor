//! 重复文件：先按大小分组，再抽样/完整 SHA256

use crate::model::FsEntry;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// 每组保留哪一份副本
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeepStrategy {
    /// 优先保留不在「下载」目录的副本（默认）
    #[default]
    PreferNotDownloads,
    /// 路径最短
    ShortestPath,
    /// 路径字典序最小
    LexFirst,
    /// 最近修改
    NewestMtime,
    /// 最早修改
    OldestMtime,
}

impl KeepStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PreferNotDownloads => "prefer_not_downloads",
            Self::ShortestPath => "shortest",
            Self::LexFirst => "lex",
            Self::NewestMtime => "newest",
            Self::OldestMtime => "oldest",
        }
    }

    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "shortest" | "short" => Self::ShortestPath,
            "lex" | "lexfirst" | "first" => Self::LexFirst,
            "newest" | "new" => Self::NewestMtime,
            "oldest" | "old" => Self::OldestMtime,
            _ => Self::PreferNotDownloads,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::PreferNotDownloads => "优先保留非下载目录",
            Self::ShortestPath => "保留路径最短",
            Self::LexFirst => "保留路径字典序最先",
            Self::NewestMtime => "保留最近修改",
            Self::OldestMtime => "保留最早修改",
        }
    }

    pub fn all() -> &'static [KeepStrategy] {
        &[
            Self::PreferNotDownloads,
            Self::ShortestPath,
            Self::LexFirst,
            Self::NewestMtime,
            Self::OldestMtime,
        ]
    }
}

#[derive(Debug, Clone)]
pub struct DupGroup {
    pub size: u64,
    pub hash: String,
    pub paths: Vec<PathBuf>,
    /// 每组默认保留一项，其余可删
    pub selected: Vec<bool>,
}

impl DupGroup {
    pub fn waste(&self) -> u64 {
        if self.paths.len() <= 1 {
            0
        } else {
            self.size * (self.paths.len() as u64 - 1)
        }
    }
}

fn path_mtime(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn looks_like_downloads(p: &Path) -> bool {
    p.components().any(|c| {
        let s = c.as_os_str().to_string_lossy().to_ascii_lowercase();
        s == "downloads" || s == "download" || s == "下载"
    })
}

fn keep_index(paths: &[PathBuf], strategy: KeepStrategy) -> usize {
    if paths.is_empty() {
        return 0;
    }
    match strategy {
        KeepStrategy::PreferNotDownloads => {
            if let Some((i, _)) = paths
                .iter()
                .enumerate()
                .filter(|(_, p)| !looks_like_downloads(p))
                .min_by_key(|(_, p)| {
                    (
                        p.to_string_lossy().len(),
                        p.to_string_lossy().to_ascii_lowercase(),
                    )
                })
            {
                i
            } else {
                paths
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, p)| p.to_string_lossy().len())
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            }
        }
        KeepStrategy::ShortestPath => paths
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| {
                (
                    p.to_string_lossy().len(),
                    p.to_string_lossy().to_ascii_lowercase(),
                )
            })
            .map(|(i, _)| i)
            .unwrap_or(0),
        KeepStrategy::LexFirst => paths
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| p.to_string_lossy().to_ascii_lowercase())
            .map(|(i, _)| i)
            .unwrap_or(0),
        KeepStrategy::NewestMtime => paths
            .iter()
            .enumerate()
            .max_by_key(|(_, p)| path_mtime(p))
            .map(|(i, _)| i)
            .unwrap_or(0),
        KeepStrategy::OldestMtime => paths
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| path_mtime(p))
            .map(|(i, _)| i)
            .unwrap_or(0),
    }
}

/// 按策略重排：保留项放第一位，并刷新 selected（仅第一项不勾选删除）
pub fn apply_keep_strategy(group: &mut DupGroup, strategy: KeepStrategy) {
    if group.paths.len() <= 1 {
        group.selected = vec![false; group.paths.len()];
        return;
    }
    let keep_idx = keep_index(&group.paths, strategy);
    if keep_idx != 0 {
        group.paths.swap(0, keep_idx);
    }
    group.selected = (0..group.paths.len()).map(|i| i != 0).collect();
}

/// 在已扫描索引中找重复文件。`min_size` 默认建议 ≥ 1MB 以控时。
pub fn find_duplicates(
    entries: &HashMap<String, FsEntry>,
    min_size: u64,
    cancel: &AtomicBool,
    max_groups: usize,
    strategy: KeepStrategy,
) -> Vec<DupGroup> {
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for e in entries.values() {
        if e.is_dir || e.size < min_size {
            continue;
        }
        by_size.entry(e.size).or_default().push(e.path.clone());
    }

    let candidates: Vec<(u64, Vec<PathBuf>)> = by_size
        .into_iter()
        .filter(|(_, v)| v.len() >= 2)
        .collect();

    let mut groups: Vec<DupGroup> = candidates
        .into_par_iter()
        .flat_map_iter(|(size, paths)| {
            if cancel.load(Ordering::Relaxed) {
                return Vec::new();
            }
            let mut by_hash: HashMap<String, Vec<PathBuf>> = HashMap::new();
            for p in paths {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                if let Some(h) = file_hash_sample(&p) {
                    by_hash.entry(h).or_default().push(p);
                }
            }
            by_hash
                .into_iter()
                .filter(|(_, v)| v.len() >= 2)
                .flat_map(|(_hash, paths)| {
                    // 只有完整哈希一致才算重复；读不了完整内容的文件直接丢弃，
                    // 绝不用抽样哈希（仅前 64KB）确认重复，否则可能误删。
                    let mut confirmed: HashMap<String, Vec<PathBuf>> = HashMap::new();
                    for p in paths {
                        if let Some(full) = file_hash_full(&p) {
                            confirmed.entry(full).or_default().push(p);
                        }
                    }
                    confirmed
                        .into_iter()
                        .filter(|(_, v)| v.len() >= 2)
                        .map(|(h, paths)| {
                            let mut g = DupGroup {
                                size,
                                hash: h,
                                paths,
                                selected: Vec::new(),
                            };
                            apply_keep_strategy(&mut g, strategy);
                            g
                        })
                        .collect::<Vec<_>>()
                })
                .collect()
        })
        .collect();

    groups.sort_by(|a, b| b.waste().cmp(&a.waste()));
    if groups.len() > max_groups {
        groups.truncate(max_groups);
    }
    groups
}

fn file_hash_sample(path: &Path) -> Option<String> {
    let mut f = File::open(path).ok()?;
    let mut buf = [0u8; 64 * 1024];
    let n = f.read(&mut buf).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&buf[..n]);
    Some(format!("{:x}", hasher.finalize()))
}

fn file_hash_full(path: &Path) -> Option<String> {
    let mut f = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn keep_prefer_not_downloads() {
        let mut g = DupGroup {
            size: 10,
            hash: "x".into(),
            paths: vec![
                PathBuf::from(r"C:\Users\a\Downloads\a.bin"),
                PathBuf::from(r"C:\Data\a.bin"),
            ],
            selected: vec![],
        };
        apply_keep_strategy(&mut g, KeepStrategy::PreferNotDownloads);
        assert_eq!(g.paths[0], PathBuf::from(r"C:\Data\a.bin"));
        assert_eq!(g.selected, vec![false, true]);
    }

    #[test]
    fn find_duplicates_empty() {
        let map = HashMap::new();
        let cancel = AtomicBool::new(false);
        let g = find_duplicates(&map, 100, &cancel, 10, KeepStrategy::default());
        assert!(g.is_empty());
    }
}
