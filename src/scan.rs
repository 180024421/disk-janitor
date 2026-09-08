//! 并行扫盘引擎（支持增量快照；跳过目录可估算占用；可续扫 / 强制展开；排除列表 / 极速）

use crate::model::{format_bytes, EstimateQuality, FsEntry, ScanIndex};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// 扫描选项：排除路径（前缀匹配，大小写不敏感）与极速模式。
#[derive(Debug, Clone, Default)]
pub struct ScanOptions {
    pub excludes: Vec<PathBuf>,
    /// 极速：不把 node_modules/.git 标为 CountOnly；提高估算上限。
    pub turbo: bool,
}

impl ScanOptions {
    pub fn quick_limit(&self) -> u64 {
        if self.turbo {
            500_000
        } else {
            80_000
        }
    }
}

/// 预规范化的排除前缀集合：构建一次，热路径只做字符串比较（大小写不敏感）。
#[derive(Debug, Clone, Default)]
pub struct ExcludeSet {
    /// 小写、`\` 分隔、无尾斜杠的前缀
    prefixes: Vec<String>,
}

impl ExcludeSet {
    pub fn new(excludes: &[PathBuf]) -> Self {
        let prefixes = excludes
            .iter()
            .filter_map(|ex| {
                let mut ek = ex.to_string_lossy().to_ascii_lowercase().replace('/', "\\");
                while ek.ends_with('\\') {
                    ek.pop();
                }
                // 盘符根「c:」补回反斜杠语义由 contains 统一处理
                if ek.is_empty() {
                    None
                } else {
                    Some(ek)
                }
            })
            .collect();
        Self { prefixes }
    }

    pub fn contains(&self, path: &Path) -> bool {
        if self.prefixes.is_empty() {
            return false;
        }
        let mut key = path
            .to_string_lossy()
            .to_ascii_lowercase()
            .replace('/', "\\");
        while key.ends_with('\\') {
            key.pop();
        }
        self.prefixes.iter().any(|ek| {
            key.len() >= ek.len()
                && key.starts_with(ek.as_str())
                && (key.len() == ek.len() || key.as_bytes()[ek.len()] == b'\\')
        })
    }
}

/// 路径是否命中排除前缀（大小写不敏感）。
pub fn path_is_excluded(path: &Path, excludes: &[PathBuf]) -> bool {
    ExcludeSet::new(excludes).contains(path)
}

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
    /// 不展开子树时的强制目录占用
    forced_dir_size: Option<u64>,
}

pub enum ScanEvent {
    Progress(ScanProgress),
    Partial(ScanIndex),
    Done(ScanIndex),
}

enum SkipKind {
    /// 完全忽略（回收站等）
    Ignore,
    /// 估算占用后记为目录条目，不进栈
    CountOnly,
}

const PARTIAL_MIN_NEW_ENTRIES: usize = 2_048;
const PARTIAL_MAX_SILENCE: Duration = Duration::from_secs(45);

/// 全新扫描（默认选项）
pub fn scan_path<F>(root: PathBuf, cancel: Arc<AtomicBool>, on_event: F) -> ScanIndex
where
    F: FnMut(ScanEvent),
{
    scan_path_ex(root, cancel, on_event, ScanOptions::default())
}

/// 带排除列表的扫描。
pub fn scan_path_filtered<F>(
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    on_event: F,
    excludes: &[PathBuf],
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
            turbo: false,
        },
    )
}

/// 全新扫描（排除列表 + 极速）
pub fn scan_path_ex<F>(
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    mut on_event: F,
    opts: ScanOptions,
) -> ScanIndex
where
    F: FnMut(ScanEvent),
{
    let started = Instant::now();
    let mut index = ScanIndex {
        root: root.clone(),
        ..Default::default()
    };

    if !root.exists() {
        let msg = format!("路径不存在: {}", root.display());
        index.errors.push(msg.clone());
        index.quality = EstimateQuality::Unavailable;
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
    let mut stack: Vec<PathBuf> = Vec::new();
    let visited0 = Arc::new(AtomicU64::new(0));
    let bytes0 = Arc::new(AtomicU64::new(0));
    let quick_lim = opts.quick_limit();

    if let Ok(meta) = std::fs::metadata(&root) {
        raws.push(Raw {
            path: root.clone(),
            is_dir: meta.is_dir(),
            file_size: if meta.is_dir() { 0 } else { meta.len() },
            mtime: meta.modified().ok(),
            forced_dir_size: None,
        });
    }
    let empty_force = HashSet::new();
    let mut skipped0 = 0u64;
    let mut skipped_bytes0 = 0u64;
    let mut skipped_notes: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&root) {
        for ent in rd.flatten() {
            let path = ent.path();
            if path_is_excluded(&path, &opts.excludes) {
                skipped0 += 1;
                continue;
            }
            if let Ok(meta) = std::fs::symlink_metadata(&path) {
                let ft = meta.file_type();
                if ft.is_symlink() {
                    continue;
                }
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if let Some(kind) = should_skip_dir(&path, name, &empty_force, opts.turbo) {
                    skipped0 += 1;
                    match kind {
                        SkipKind::Ignore => continue,
                        SkipKind::CountOnly => {
                            let (sz, _) = quick_dir_size(&path, cancel.as_ref(), quick_lim);
                            skipped_bytes0 += sz;
                            bytes0.fetch_add(sz, Ordering::Relaxed);
                            visited0.fetch_add(1, Ordering::Relaxed);
                            if skipped_notes.len() < 30 {
                                skipped_notes.push(format!(
                                    "{} ≈ {}（未展开）",
                                    path.display(),
                                    format_bytes(sz)
                                ));
                            }
                            raws.push(Raw {
                                path,
                                is_dir: true,
                                file_size: 0,
                                mtime: meta.modified().ok(),
                                forced_dir_size: Some(sz),
                            });
                            continue;
                        }
                    }
                }
                let is_dir = ft.is_dir();
                let file_size = if is_dir { 0 } else { meta.len() };
                if is_dir {
                    stack.push(path.clone());
                } else {
                    bytes0.fetch_add(file_size, Ordering::Relaxed);
                }
                visited0.fetch_add(1, Ordering::Relaxed);
                raws.push(Raw {
                    path,
                    is_dir,
                    file_size,
                    mtime: meta.modified().ok(),
                    forced_dir_size: None,
                });
            }
        }
        // 小目录通常会立即完成，提前构建完整索引只会制造一次重复峰值。
        // 仅在根目录已显示出较大工作量时提供首个可浏览快照。
        if should_emit_initial_partial(raws.len(), stack.len()) {
            let snap = finish_counts(build_index(
                index.clone(),
                &raws,
                &root,
                skipped0,
                skipped_bytes0,
                &skipped_notes,
                true,
            ));
            on_event(ScanEvent::Partial(snap));
        }
    }

    run_scan_loop(
        root,
        index,
        raws,
        stack,
        HashSet::new(),
        visited0,
        Arc::new(AtomicU64::new(skipped0)),
        bytes0,
        Arc::new(AtomicU64::new(skipped_bytes0)),
        skipped_notes,
        started,
        cancel,
        opts,
        on_event,
    )
}

/// 从取消时留下的剩余目录栈继续扫，合并进 base_index
pub fn resume_scan<F>(
    root: PathBuf,
    base_index: ScanIndex,
    remaining: Vec<PathBuf>,
    cancel: Arc<AtomicBool>,
    on_event: F,
) -> ScanIndex
where
    F: FnMut(ScanEvent),
{
    resume_scan_ex(
        root,
        base_index,
        remaining,
        cancel,
        on_event,
        ScanOptions::default(),
    )
}

pub fn resume_scan_ex<F>(
    root: PathBuf,
    base_index: ScanIndex,
    remaining: Vec<PathBuf>,
    cancel: Arc<AtomicBool>,
    on_event: F,
    opts: ScanOptions,
) -> ScanIndex
where
    F: FnMut(ScanEvent),
{
    let started = Instant::now();
    let raws = raws_from_index(&base_index);
    let skipped = Arc::new(AtomicU64::new(base_index.skipped));
    let skipped_bytes = Arc::new(AtomicU64::new(base_index.skipped_bytes));
    let notes = base_index.skipped_notes.clone();
    let (visited, bytes_seen) = counters_from_raws(&raws);
    let mut index = base_index;
    index.root = root.clone();
    index.resume_stack.clear();
    index.partial = true;

    run_scan_loop(
        root,
        index,
        raws,
        remaining,
        HashSet::new(),
        visited,
        skipped,
        bytes_seen,
        skipped_bytes,
        notes,
        started,
        cancel,
        opts,
        on_event,
    )
}

/// 强制展开此前 CountOnly 的目录，合并进 root_index，清除该目录 count_only 并重算占用
pub fn expand_count_only_dir<F>(
    root_index: ScanIndex,
    expand_dir: PathBuf,
    cancel: Arc<AtomicBool>,
    on_event: F,
) -> ScanIndex
where
    F: FnMut(ScanEvent),
{
    let started = Instant::now();
    let root = root_index.root.clone();
    let expand_key = ScanIndex::key_norm(&expand_dir);
    let expand_prefix = {
        let base = expand_key.trim_end_matches(['\\', '/']);
        format!("{base}\\")
    };

    let was = root_index.get(&expand_dir).cloned();
    let mut skipped_n = root_index.skipped;
    let mut skipped_b = root_index.skipped_bytes;
    let mut notes = root_index.skipped_notes.clone();
    if was.as_ref().map(|e| e.count_only).unwrap_or(false) {
        skipped_n = skipped_n.saturating_sub(1);
        skipped_b = skipped_b.saturating_sub(was.as_ref().map(|e| e.size).unwrap_or(0));
        let marker = expand_dir.display().to_string();
        notes.retain(|n| !n.contains(&marker));
    }

    let mut raws = raws_from_index(&root_index);
    raws.retain(|r| {
        let k = ScanIndex::key_norm(&r.path);
        k == expand_key || !k.starts_with(&expand_prefix)
    });
    for r in &mut raws {
        if ScanIndex::key_norm(&r.path) == expand_key {
            r.forced_dir_size = None;
            r.file_size = 0;
        }
    }
    if !raws
        .iter()
        .any(|r| ScanIndex::key_norm(&r.path) == expand_key)
    {
        if let Ok(meta) = std::fs::metadata(&expand_dir) {
            raws.push(Raw {
                path: expand_dir.clone(),
                is_dir: meta.is_dir(),
                file_size: 0,
                mtime: meta.modified().ok(),
                forced_dir_size: None,
            });
        }
    }

    let mut force_set = HashSet::new();
    force_set.insert(expand_key);

    let (visited, bytes_seen) = counters_from_raws(&raws);
    let mut index = root_index;
    index.resume_stack.clear();
    index.partial = true;

    run_scan_loop(
        root,
        index,
        raws,
        vec![expand_dir],
        force_set,
        visited,
        Arc::new(AtomicU64::new(skipped_n)),
        bytes_seen,
        Arc::new(AtomicU64::new(skipped_b)),
        notes,
        started,
        cancel,
        ScanOptions::default(),
        on_event,
    )
}

fn counters_from_raws(raws: &[Raw]) -> (Arc<AtomicU64>, Arc<AtomicU64>) {
    let visited = raws.len() as u64;
    let bytes: u64 = raws
        .iter()
        .map(|r| r.forced_dir_size.unwrap_or(r.file_size))
        .sum();
    (
        Arc::new(AtomicU64::new(visited)),
        Arc::new(AtomicU64::new(bytes)),
    )
}

fn raws_from_index(index: &ScanIndex) -> Vec<Raw> {
    index
        .entries
        .values()
        .map(|e| Raw {
            path: e.path.clone(),
            is_dir: e.is_dir,
            file_size: if e.is_dir { 0 } else { e.size },
            mtime: e.mtime,
            forced_dir_size: if e.count_only { Some(e.size) } else { None },
        })
        .collect()
}

fn run_scan_loop<F>(
    root: PathBuf,
    mut index: ScanIndex,
    mut raws: Vec<Raw>,
    mut stack: Vec<PathBuf>,
    force_set: HashSet<String>,
    visited: Arc<AtomicU64>,
    skipped: Arc<AtomicU64>,
    bytes_seen: Arc<AtomicU64>,
    skipped_bytes: Arc<AtomicU64>,
    mut skipped_notes: Vec<String>,
    started: Instant,
    cancel: Arc<AtomicBool>,
    opts: ScanOptions,
    mut on_event: F,
) -> ScanIndex
where
    F: FnMut(ScanEvent),
{
    let mut last_ui = Instant::now();
    let mut last_partial = Instant::now();
    let mut last_partial_entries = raws.len();
    let quick_lim = opts.quick_limit();
    let turbo = opts.turbo;
    let excludes = ExcludeSet::new(&opts.excludes);

    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            let skipped_n = skipped.load(Ordering::Relaxed);
            let sb = skipped_bytes.load(Ordering::Relaxed);
            let mut resume = stack;
            resume.push(dir.clone());
            let mut done = finish_counts(build_index(
                index,
                &raws,
                &root,
                skipped_n,
                sb,
                &skipped_notes,
                true,
            ));
            done.resume_stack = resume;
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

        if excludes.contains(&dir) {
            skipped.fetch_add(1, Ordering::Relaxed);
            continue;
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
        let force_ref = &force_set;
        let excludes_ref = &excludes;
        let chunk: Vec<Raw> = children
            .par_iter()
            .filter_map(|path| {
                // 当前目录已经出栈；即使此时收到取消，也应登记完这一批，
                // 否则被跳过的子目录无法进入 resume_stack。
                if excludes_ref.contains(path) {
                    skipped.fetch_add(1, Ordering::Relaxed);
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
                        forced_dir_size: None,
                    });
                }
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if let Some(kind) = should_skip_dir(path, name, force_ref, turbo) {
                    skipped.fetch_add(1, Ordering::Relaxed);
                    match kind {
                        SkipKind::Ignore => return None,
                        SkipKind::CountOnly => {
                            let (sz, _) = quick_dir_size(path, cancel.as_ref(), quick_lim);
                            skipped_bytes.fetch_add(sz, Ordering::Relaxed);
                            bytes_seen.fetch_add(sz, Ordering::Relaxed);
                            visited.fetch_add(1, Ordering::Relaxed);
                            return Some(Raw {
                                path: path.clone(),
                                is_dir: true,
                                file_size: 0,
                                mtime: meta.modified().ok(),
                                forced_dir_size: Some(sz),
                            });
                        }
                    }
                }
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
                    forced_dir_size: None,
                })
            })
            .collect();

        for r in &chunk {
            if let Some(sz) = r.forced_dir_size {
                if skipped_notes.len() < 30 {
                    skipped_notes.push(format!(
                        "{} ≈ {}（未展开）",
                        r.path.display(),
                        format_bytes(sz)
                    ));
                }
            } else if r.is_dir {
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

        let n = raws.len();
        if should_emit_partial(n, last_partial_entries, last_partial.elapsed()) {
            last_partial = Instant::now();
            last_partial_entries = n;
            let skipped_n = skipped.load(Ordering::Relaxed);
            let sb = skipped_bytes.load(Ordering::Relaxed);
            let snap = finish_counts(build_index(
                ScanIndex {
                    root: root.clone(),
                    errors: index.errors.clone(),
                    ..Default::default()
                },
                &raws,
                &root,
                skipped_n,
                sb,
                &skipped_notes,
                true,
            ));
            on_event(ScanEvent::Partial(snap));
        }
    }

    let skipped_n = skipped.load(Ordering::Relaxed);
    let sb = skipped_bytes.load(Ordering::Relaxed);
    let cancelled = cancel.load(Ordering::Relaxed);
    let mut done = finish_counts(build_index(
        index,
        &raws,
        &root,
        skipped_n,
        sb,
        &skipped_notes,
        cancelled,
    ));
    if !cancelled {
        done.resume_stack.clear();
    }
    on_event(ScanEvent::Progress(ScanProgress {
        visited: visited.load(Ordering::Relaxed),
        skipped: skipped_n,
        bytes_seen: bytes_seen.load(Ordering::Relaxed),
        current: root.display().to_string(),
        elapsed: started.elapsed(),
        done: true,
        cancelled,
        error: None,
    }));
    on_event(ScanEvent::Done(done.clone()));
    done
}

fn should_emit_initial_partial(entries: usize, pending_dirs: usize) -> bool {
    pending_dirs > 0 && (entries >= 2_000 || pending_dirs >= 64)
}

fn should_emit_partial(entries: usize, previous_entries: usize, elapsed: Duration) -> bool {
    let new_entries = entries.saturating_sub(previous_entries);
    if new_entries == 0 {
        return false;
    }
    let interval = if entries > 120_000 {
        Duration::from_secs(30)
    } else if entries > 80_000 {
        Duration::from_secs(20)
    } else if entries > 20_000 {
        Duration::from_secs(10)
    } else {
        Duration::from_secs(5)
    };
    let proportional_growth = previous_entries / 10;
    let meaningful_growth = PARTIAL_MIN_NEW_ENTRIES.max(proportional_growth);
    elapsed >= interval && (new_entries >= meaningful_growth || elapsed >= PARTIAL_MAX_SILENCE)
}

fn finish_counts(mut index: ScanIndex) -> ScanIndex {
    index.rebuild_children();
    index.file_count = index.entries.values().filter(|e| !e.is_dir).count() as u64;
    index.dir_count = index.entries.values().filter(|e| e.is_dir).count() as u64;
    index.quality = if index.partial {
        EstimateQuality::Estimated
    } else if !index.errors.is_empty() {
        EstimateQuality::PermissionLimited
    } else if index.skipped_bytes > 0 {
        EstimateQuality::Estimated
    } else {
        EstimateQuality::Complete
    };
    index
}

fn should_skip_dir(
    path: &Path,
    name: &str,
    force_set: &HashSet<String>,
    turbo: bool,
) -> Option<SkipKind> {
    if force_set.contains(&ScanIndex::key_norm(path)) {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "$recycle.bin" | "system volume information" | "csc" => Some(SkipKind::Ignore),
        "node_modules" | ".git" | ".svn" if turbo => None,
        "winsxs"
        | "installer"
        | "servicing"
        | "softwaredistribution"
        | "package cache"
        | "node_modules"
        | ".git"
        | ".svn"
        | "onedrive"
        | "onedrivetemp" => Some(SkipKind::CountOnly),
        _ if lower.starts_with("onedrive") => Some(SkipKind::CountOnly),
        _ => None,
    }
}

fn build_index(
    mut index: ScanIndex,
    raws: &[Raw],
    root: &Path,
    skipped: u64,
    skipped_bytes: u64,
    skipped_notes: &[String],
    partial: bool,
) -> ScanIndex {
    index.skipped = skipped;
    index.skipped_bytes = skipped_bytes;
    index.skipped_notes = skipped_notes.to_vec();
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
            key.clone(),
            FsEntry {
                path: r.path.clone(),
                name,
                is_dir: r.is_dir,
                size: r.file_size,
                mtime: r.mtime,
                count_only: r.forced_dir_size.is_some(),
            },
        );
        let add = if let Some(fs) = r.forced_dir_size {
            fs
        } else if !r.is_dir && r.file_size > 0 {
            r.file_size
        } else {
            0
        };
        if add > 0 {
            if r.forced_dir_size.is_some() {
                *dir_sizes.entry(key).or_insert(0) += add;
            }
            for anc in r.path.ancestors().skip(1) {
                if anc.as_os_str().is_empty() {
                    break;
                }
                if !anc.starts_with(root) {
                    continue;
                }
                let k = ScanIndex::key(anc);
                *dir_sizes.entry(k).or_insert(0) += add;
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

pub fn quick_dir_size(path: &Path, cancel: &AtomicBool, max_files: u64) -> (u64, u64) {
    let (bytes, files, _) = quick_dir_size_with_status(path, cancel, max_files);
    (bytes, files)
}

pub fn quick_dir_size_with_status(
    path: &Path,
    cancel: &AtomicBool,
    max_files: u64,
) -> (u64, u64, bool) {
    let mut total = 0u64;
    let mut files = 0u64;
    let mut stack = vec![path.to_path_buf()];
    let mut complete = true;
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            complete = false;
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            if cancel.load(Ordering::Relaxed) || files >= max_files {
                complete = false;
                return (total, files, complete);
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
    (total, files, complete)
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
    fn counts_skipped_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir_all(&nm).unwrap();
        fs::write(nm.join("pkg.js"), vec![1u8; 2000]).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let idx = scan_path(dir.path().to_path_buf(), cancel, |_| {});
        assert!(idx.skipped_bytes >= 2000 || idx.get(&nm).map(|e| e.size).unwrap_or(0) >= 2000);
        assert_eq!(idx.get(dir.path()).unwrap().size, 2000);
        assert!(
            idx.get(&nm).map(|e| e.count_only).unwrap_or(false),
            "node_modules should be count_only"
        );
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

    #[test]
    fn exclude_prefix_skips_subtree() {
        let dir = tempfile::tempdir().unwrap();
        let keep = dir.path().join("keep");
        let skip = dir.path().join("skip_me");
        fs::create_dir_all(&keep).unwrap();
        fs::create_dir_all(&skip).unwrap();
        fs::write(keep.join("a.txt"), vec![1u8; 100]).unwrap();
        fs::write(skip.join("b.txt"), vec![1u8; 500]).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let excludes = vec![skip.clone()];
        let idx = scan_path_filtered(dir.path().to_path_buf(), cancel, |_| {}, &excludes);
        assert!(idx.get(&skip).is_none());
        assert_eq!(idx.get(dir.path()).unwrap().size, 100);
    }

    #[test]
    fn path_exclude_case_insensitive() {
        let p = PathBuf::from(r"C:\Users\Foo\Bar");
        let ex = vec![PathBuf::from(r"c:\users\foo")];
        assert!(path_is_excluded(&p, &ex));
        assert!(!path_is_excluded(&PathBuf::from(r"C:\Users\Other"), &ex));
    }

    #[test]
    fn exclude_prefix_respects_path_boundary() {
        let ex = vec![PathBuf::from(r"C:\Users\Foo")];
        let set = ExcludeSet::new(&ex);
        assert!(set.contains(Path::new(r"c:\users\foo")));
        assert!(set.contains(Path::new(r"C:\Users\Foo\sub\a.txt")));
        // 同前缀但不同目录不应命中
        assert!(!set.contains(Path::new(r"C:\Users\FooBar")));
        // 正斜杠也应规范化
        assert!(set.contains(Path::new("C:/Users/Foo/x")));
    }

    #[test]
    fn partial_snapshots_require_time_and_meaningful_growth() {
        assert!(!should_emit_partial(
            20_000,
            20_000,
            Duration::from_secs(60)
        ));
        assert!(!should_emit_partial(
            21_000,
            20_000,
            Duration::from_secs(10)
        ));
        assert!(should_emit_partial(23_000, 20_000, Duration::from_secs(10)));
        assert!(should_emit_partial(20_100, 20_000, PARTIAL_MAX_SILENCE));
    }

    #[test]
    fn small_root_does_not_build_redundant_initial_snapshot() {
        assert!(!should_emit_initial_partial(20, 3));
        assert!(should_emit_initial_partial(2_000, 1));
        assert!(should_emit_initial_partial(100, 64));
    }

    #[test]
    fn cancelled_scan_resumes_all_pending_directories_in_temp_directory() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("a.txt"), b"a").unwrap();
        fs::write(b.join("b.txt"), b"bb").unwrap();

        let paused = scan_path(
            dir.path().to_path_buf(),
            Arc::new(AtomicBool::new(true)),
            |_| {},
        );
        assert!(paused.partial);
        assert_eq!(paused.resume_stack.len(), 2);

        let remaining = paused.resume_stack.clone();
        let resumed = resume_scan(
            dir.path().to_path_buf(),
            paused,
            remaining,
            Arc::new(AtomicBool::new(false)),
            |_| {},
        );
        assert!(!resumed.partial);
        assert!(resumed.resume_stack.is_empty());
        assert_eq!(resumed.get(dir.path()).unwrap().size, 3);
        assert!(resumed.get(&a.join("a.txt")).is_some());
        assert!(resumed.get(&b.join("b.txt")).is_some());
    }
}
