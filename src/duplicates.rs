//! 重复文件：先按大小分组，再抽样/完整 SHA256

use crate::model::FsEntry;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// 删除前的单文件重验结果。
///
/// 调用方应在真正删除前尽可能晚地调用 [`prepare_group_deletions`]，以缩小
/// 重验与删除之间的竞态窗口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevalidationStatus {
    Valid,
    Missing,
    NotAFile,
    SizeChanged { expected: u64, actual: u64 },
    HashChanged { expected: String, actual: String },
    Unreadable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRevalidation {
    pub path: PathBuf,
    pub status: RevalidationStatus,
}

/// 逐组确认并重验删除候选时可能出现的安全阻断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteSafetyError {
    /// 即使界面已经勾选文件，也必须再明确确认当前重复组。
    GroupConfirmationRequired,
    /// 不允许删除组内全部路径。
    MustKeepAtLeastOne,
    /// 至少一个候选已不存在、长度变化、哈希变化或无法读取。
    RevalidationFailed(Vec<FileRevalidation>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

/// 当前项目没有媒体探测依赖，因此不猜测时长。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDuration {
    NotMedia,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionalFileMetadata {
    pub image_dimensions: Option<ImageDimensions>,
    pub media_duration: MediaDuration,
}

#[derive(Debug, Clone)]
pub struct FolderPairDupGroup {
    pub duplicate: DupGroup,
    pub first_paths: Vec<PathBuf>,
    pub second_paths: Vec<PathBuf>,
}

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
        let physical_files = distinct_physical_file_count(&self.paths);
        if physical_files <= 1 {
            0
        } else {
            self.size
                .saturating_mul(physical_files.saturating_sub(1) as u64)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PhysicalFileId {
    #[cfg(windows)]
    Windows { volume: u32, index: u64 },
    #[cfg(unix)]
    Unix { device: u64, inode: u64 },
    /// 不支持或无法取得文件标识时，保守地把每条不同路径视为独立文件。
    Path(PathBuf),
}

fn physical_file_id(path: &Path) -> PhysicalFileId {
    physical_file_id_result(path).unwrap_or_else(|_| PhysicalFileId::Path(path.to_path_buf()))
}

fn physical_file_id_result(path: &Path) -> std::io::Result<PhysicalFileId> {
    let metadata = fs::metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if let (Some(volume), Some(index)) =
            (metadata.volume_serial_number(), metadata.file_index())
        {
            return Ok(PhysicalFileId::Windows { volume, index });
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return Ok(PhysicalFileId::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        });
    }

    #[allow(unreachable_code)]
    Ok(PhysicalFileId::Path(fs::canonicalize(path)?))
}

/// 判断两个路径是否指向同一物理文件（Windows 使用卷序列号 + 文件索引）。
pub fn same_physical_file(first: &Path, second: &Path) -> std::io::Result<bool> {
    Ok(physical_file_id_result(first)? == physical_file_id_result(second)?)
}

fn distinct_physical_file_count(paths: &[PathBuf]) -> usize {
    paths
        .iter()
        .map(|path| physical_file_id(path))
        .collect::<HashSet<_>>()
        .len()
}

fn path_is_within(path: &Path, folder: &Path) -> bool {
    #[cfg(windows)]
    {
        let normalize = |value: &Path| {
            value
                .to_string_lossy()
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_ascii_lowercase()
        };
        let path = normalize(path);
        let folder = normalize(folder);
        return path == folder || path.starts_with(&format!("{folder}\\"));
    }
    #[cfg(not(windows))]
    {
        path.starts_with(folder)
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

    let candidates: Vec<(u64, Vec<PathBuf>)> =
        by_size.into_iter().filter(|(_, v)| v.len() >= 2).collect();

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

/// 仅返回同时跨越两个文件夹的重复组，并标明每一侧的路径。
///
/// 同一组仍沿用 [`DupGroup::selected`]，调用方不得据此直接删除；删除前必须
/// 对每组单独调用 [`prepare_group_deletions`] 并传入明确确认。
pub fn find_duplicates_between_folders(
    entries: &HashMap<String, FsEntry>,
    first_folder: &Path,
    second_folder: &Path,
    min_size: u64,
    cancel: &AtomicBool,
    max_groups: usize,
    strategy: KeepStrategy,
) -> Vec<FolderPairDupGroup> {
    if path_is_within(first_folder, second_folder) || path_is_within(second_folder, first_folder) {
        return Vec::new();
    }

    let scoped: HashMap<String, FsEntry> = entries
        .iter()
        .filter(|(_, entry)| {
            !entry.is_dir
                && (path_is_within(&entry.path, first_folder)
                    || path_is_within(&entry.path, second_folder))
        })
        .map(|(key, entry)| (key.clone(), entry.clone()))
        .collect();

    find_duplicates(&scoped, min_size, cancel, usize::MAX, strategy)
        .into_iter()
        .filter_map(|duplicate| {
            let first_paths: Vec<PathBuf> = duplicate
                .paths
                .iter()
                .filter(|path| path_is_within(path, first_folder))
                .cloned()
                .collect();
            let second_paths: Vec<PathBuf> = duplicate
                .paths
                .iter()
                .filter(|path| path_is_within(path, second_folder))
                .cloned()
                .collect();
            if first_paths.is_empty() || second_paths.is_empty() {
                None
            } else {
                Some(FolderPairDupGroup {
                    duplicate,
                    first_paths,
                    second_paths,
                })
            }
        })
        .take(max_groups)
        .collect()
}

/// 只需要原有分组结构时使用的文件夹成对比较便捷 API。
pub fn find_duplicates_in_folder_pair(
    entries: &HashMap<String, FsEntry>,
    first_folder: &Path,
    second_folder: &Path,
    min_size: u64,
    cancel: &AtomicBool,
    max_groups: usize,
    strategy: KeepStrategy,
) -> Vec<DupGroup> {
    find_duplicates_between_folders(
        entries,
        first_folder,
        second_folder,
        min_size,
        cancel,
        max_groups,
        strategy,
    )
    .into_iter()
    .map(|group| group.duplicate)
    .collect()
}

/// 重验文件仍存在、仍是普通文件、长度未变且完整 SHA256 未变。
pub fn revalidate_duplicate_file(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> FileRevalidation {
    let status = match fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => RevalidationStatus::Missing,
        Err(error) => RevalidationStatus::Unreadable(error.to_string()),
        Ok(metadata) if !metadata.is_file() => RevalidationStatus::NotAFile,
        Ok(metadata) if metadata.len() != expected_size => RevalidationStatus::SizeChanged {
            expected: expected_size,
            actual: metadata.len(),
        },
        Ok(_) => match file_hash_full_result(path) {
            Err(error) => RevalidationStatus::Unreadable(error.to_string()),
            Ok(actual) if !actual.eq_ignore_ascii_case(expected_sha256) => {
                RevalidationStatus::HashChanged {
                    expected: expected_sha256.to_ascii_lowercase(),
                    actual,
                }
            }
            Ok(_) => match fs::metadata(path) {
                Ok(metadata) if metadata.is_file() && metadata.len() == expected_size => {
                    RevalidationStatus::Valid
                }
                Ok(metadata) if !metadata.is_file() => RevalidationStatus::NotAFile,
                Ok(metadata) => RevalidationStatus::SizeChanged {
                    expected: expected_size,
                    actual: metadata.len(),
                },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    RevalidationStatus::Missing
                }
                Err(error) => RevalidationStatus::Unreadable(error.to_string()),
            },
        },
    };
    FileRevalidation {
        path: path.to_path_buf(),
        status,
    }
}

/// 为一个已人工确认的组生成可删除路径；任何重验失败都会阻断整个组。
pub fn prepare_group_deletions(
    group: &DupGroup,
    group_confirmed: bool,
) -> Result<Vec<PathBuf>, DeleteSafetyError> {
    let selected: Vec<&PathBuf> = group
        .paths
        .iter()
        .enumerate()
        .filter(|(index, _)| group.selected.get(*index).copied().unwrap_or(false))
        .map(|(_, path)| path)
        .collect();
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    if !group_confirmed {
        return Err(DeleteSafetyError::GroupConfirmationRequired);
    }
    if selected.len() >= group.paths.len() {
        return Err(DeleteSafetyError::MustKeepAtLeastOne);
    }

    // 连同保留项一起重验，避免保留项已丢失/变化后仍删除最后一份有效副本。
    let validations: Vec<FileRevalidation> = group
        .paths
        .iter()
        .map(|path| revalidate_duplicate_file(path, group.size, &group.hash))
        .collect();
    if validations
        .iter()
        .any(|result| result.status != RevalidationStatus::Valid)
    {
        return Err(DeleteSafetyError::RevalidationFailed(validations));
    }
    Ok(selected.into_iter().cloned().collect())
}

/// 获取轻量可选元数据。图片只读取格式解码器提供的尺寸；媒体时长明确不支持。
pub fn optional_file_metadata(path: &Path) -> OptionalFileMetadata {
    let image_dimensions = image::image_dimensions(path)
        .ok()
        .map(|(width, height)| ImageDimensions { width, height });
    let is_media = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "mp3"
                    | "wav"
                    | "flac"
                    | "aac"
                    | "m4a"
                    | "ogg"
                    | "mp4"
                    | "mkv"
                    | "avi"
                    | "mov"
                    | "webm"
                    | "wmv"
            )
        })
        .unwrap_or(false);
    OptionalFileMetadata {
        image_dimensions,
        media_duration: if is_media {
            MediaDuration::Unsupported
        } else {
            MediaDuration::NotMedia
        },
    }
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
    file_hash_full_result(path).ok()
}

fn file_hash_full_result(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use tempfile::tempdir;

    fn entry(path: &Path) -> FsEntry {
        FsEntry {
            path: path.to_path_buf(),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            size: fs::metadata(path).unwrap().len(),
            ..FsEntry::default()
        }
    }

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

    #[test]
    fn temporary_files_are_revalidated_before_deletion() {
        let temp = tempdir().unwrap();
        let keep = temp.path().join("keep.bin");
        let remove = temp.path().join("remove.bin");
        fs::write(&keep, b"same bytes").unwrap();
        fs::write(&remove, b"same bytes").unwrap();
        let hash = file_hash_full(&keep).unwrap();
        let mut group = DupGroup {
            size: 10,
            hash,
            paths: vec![keep, remove.clone()],
            selected: vec![false, true],
        };

        assert_eq!(
            prepare_group_deletions(&group, false),
            Err(DeleteSafetyError::GroupConfirmationRequired)
        );
        assert_eq!(
            prepare_group_deletions(&group, true).unwrap(),
            vec![remove.clone()]
        );

        fs::write(&remove, b"same byteS").unwrap();
        assert!(matches!(
            prepare_group_deletions(&group, true),
            Err(DeleteSafetyError::RevalidationFailed(_))
        ));

        group.selected = vec![true, true];
        assert_eq!(
            prepare_group_deletions(&group, true),
            Err(DeleteSafetyError::MustKeepAtLeastOne)
        );
    }

    #[test]
    fn compares_two_temporary_folders_as_pairs() {
        let temp = tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let a = first.join("a.bin");
        let b = second.join("b.bin");
        let only_first = first.join("c.bin");
        fs::write(&a, b"shared").unwrap();
        fs::write(&b, b"shared").unwrap();
        fs::write(&only_first, b"private").unwrap();
        let entries = [a.clone(), b.clone(), only_first]
            .into_iter()
            .map(|path| (path.to_string_lossy().into_owned(), entry(&path)))
            .collect();

        let groups = find_duplicates_between_folders(
            &entries,
            &first,
            &second,
            1,
            &AtomicBool::new(false),
            10,
            KeepStrategy::LexFirst,
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].first_paths, vec![a]);
        assert_eq!(groups[0].second_paths, vec![b]);
    }

    #[test]
    fn hard_links_do_not_inflate_reclaimable_bytes() {
        let temp = tempdir().unwrap();
        let original = temp.path().join("original.bin");
        let link = temp.path().join("link.bin");
        fs::write(&original, b"physical file").unwrap();
        fs::hard_link(&original, &link).unwrap();
        let group = DupGroup {
            size: fs::metadata(&original).unwrap().len(),
            hash: file_hash_full(&original).unwrap(),
            paths: vec![original, link],
            selected: vec![false, true],
        };
        assert_eq!(group.waste(), 0);
    }

    #[test]
    fn reads_optional_png_dimensions_and_marks_media_unsupported() {
        let temp = tempdir().unwrap();
        let png = temp.path().join("tiny.png");
        image::RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255]))
            .save(&png)
            .unwrap();
        assert_eq!(
            optional_file_metadata(&png).image_dimensions,
            Some(ImageDimensions {
                width: 3,
                height: 2
            })
        );
        assert_eq!(
            optional_file_metadata(&temp.path().join("clip.mp4")).media_duration,
            MediaDuration::Unsupported
        );
    }
}
