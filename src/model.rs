//! 文件系统扫描结果模型

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EstimateQuality {
    #[default]
    Complete,
    Estimated,
    Truncated,
    PermissionLimited,
    Unavailable,
}

impl EstimateQuality {
    pub fn label(self) -> &'static str {
        match self {
            Self::Complete => "完整",
            Self::Estimated => "估算",
            Self::Truncated => "达到上限",
            Self::PermissionLimited => "权限受限",
            Self::Unavailable => "不可用",
        }
    }
}

mod mtime_serde {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(t: &Option<SystemTime>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let v = t
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        Option::<u64>::serialize(&v, s)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Option<SystemTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Option::<u64>::deserialize(d)?;
        // 损坏/篡改缓存里的超大值不能 panic，否则绕过 corrupt 保全流程。
        v.map(|secs| {
            UNIX_EPOCH.checked_add(Duration::from_secs(secs)).ok_or_else(|| {
                serde::de::Error::custom(format!("mtime out of range: {secs}"))
            })
        })
        .transpose()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    /// 文件：自身大小；目录：下属文件合计
    pub size: u64,
    #[serde(default, with = "mtime_serde")]
    pub mtime: Option<SystemTime>,
    /// 因大目录策略未展开，仅估算占用
    #[serde(default)]
    pub count_only: bool,
}

impl Default for FsEntry {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            name: String::new(),
            is_dir: false,
            size: 0,
            mtime: None,
            count_only: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanIndex {
    pub root: PathBuf,
    /// 规范化路径字符串 -> 条目
    pub entries: HashMap<String, FsEntry>,
    /// 父路径(小写) -> 子条目 key 列表（避免每次 O(全表) 找子项）
    #[serde(skip)]
    pub children: HashMap<String, Vec<String>>,
    pub skipped: u64,
    /// 因跳过深入扫描而估算计入的字节（node_modules 等）
    pub skipped_bytes: u64,
    /// 跳过说明（供 UI 提示）
    pub skipped_notes: Vec<String>,
    pub file_count: u64,
    pub dir_count: u64,
    pub errors: Vec<String>,
    /// 扫描是否仍在进行（增量快照时为 true）
    pub partial: bool,
    /// 取消时尚未扫描的目录（用于断点续扫）
    #[serde(default)]
    pub resume_stack: Vec<PathBuf>,
    /// 扫描结果是否完整，避免把估算/截断结果展示为精确值。
    #[serde(default)]
    pub quality: EstimateQuality,
}

impl ScanIndex {
    pub fn key(path: &Path) -> String {
        let s = path.to_string_lossy();
        let t = s.trim_end_matches(['\\', '/']);
        if t.len() == 2 && t.as_bytes()[1] == b':' {
            format!("{}\\", t)
        } else {
            t.to_string()
        }
    }

    pub fn key_norm(path: &Path) -> String {
        Self::key(path).to_ascii_lowercase()
    }

    pub fn get(&self, path: &Path) -> Option<&FsEntry> {
        self.entries.get(&Self::key(path)).or_else(|| {
            let want = Self::key_norm(path);
            self.entries
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(&want))
                .map(|(_, v)| v)
        })
    }

    /// 重建父子索引（扫盘结束 / 增量快照后调用）
    pub fn rebuild_children(&mut self) {
        self.children.clear();
        self.children.reserve(self.entries.len() / 4 + 16);
        for (key, e) in &self.entries {
            let Some(parent) = e.path.parent() else {
                continue;
            };
            let pk = Self::key_norm(parent);
            let ek = Self::key_norm(&e.path);
            if pk == ek {
                continue;
            }
            self.children.entry(pk).or_default().push(key.clone());
        }
    }

    pub fn children_of(&self, dir: &Path) -> Vec<&FsEntry> {
        let parent = Self::key_norm(dir);
        let Some(keys) = self.children.get(&parent) else {
            return Vec::new();
        };
        keys.iter().filter_map(|k| self.entries.get(k)).collect()
    }

    pub fn child_dirs_of(&self, dir: &Path) -> Vec<&FsEntry> {
        self.children_of(dir)
            .into_iter()
            .filter(|e| e.is_dir)
            .collect()
    }

    pub fn child_dirs_top(&self, dir: &Path, n: usize) -> Vec<&FsEntry> {
        let mut v = self.child_dirs_of(dir);
        v.sort_by(|a, b| b.size.cmp(&a.size));
        if v.len() > n {
            v.truncate(n);
        }
        v
    }

    pub fn top_by_size(&self, dirs: bool, n: usize) -> Vec<&FsEntry> {
        let mut v: Vec<&FsEntry> = self
            .entries
            .values()
            .filter(|e| e.is_dir == dirs && e.path != self.root)
            .collect();
        v.sort_by(|a, b| b.size.cmp(&a.size));
        v.truncate(n);
        v
    }

    pub fn empty_dirs(&self, n: usize) -> Vec<&FsEntry> {
        let mut v: Vec<&FsEntry> = self
            .entries
            .values()
            .filter(|e| e.is_dir && e.size == 0 && e.path != self.root)
            .collect();
        v.sort_by(|a, b| a.path.cmp(&b.path));
        v.truncate(n);
        v
    }

    /// 删除路径及其子孙，并从祖先目录扣减占用
    pub fn remove_cascade(&mut self, path: &Path) {
        self.remove_cascade_batch(std::iter::once(path));
    }

    /// 批量级联删除：一次扫描找出全部子孙并重建子索引。
    /// 删除 N 项时避免逐项 O(entries) 扫描 + 逐项 rebuild_children 的卡顿。
    pub fn remove_cascade_batch<'a, P>(&mut self, paths: impl IntoIterator<Item = &'a P>)
    where
        P: AsRef<Path> + ?Sized + 'a,
    {
        // (小写精确 key, 小写子树前缀)
        let mut targets: Vec<(String, String)> = Vec::new();
        let mut seen_del: std::collections::HashSet<String> = std::collections::HashSet::new();
        for p in paths {
            let key_l = Self::key(p.as_ref()).to_ascii_lowercase();
            if key_l.is_empty() || !seen_del.insert(key_l.clone()) {
                continue;
            }
            let prefix = if key_l.ends_with('\\') {
                key_l.clone()
            } else {
                format!("{key_l}\\")
            };
            targets.push((key_l, prefix));
        }
        if targets.is_empty() {
            return;
        }
        // 一次遍历收集待删项；命中项（未被其它删除项覆盖的）体积沿祖先链扣减，
        // 与逐项 remove_cascade 语义一致（盘符根 key 需二次归一才能命中）。
        let mut doomed: Vec<String> = Vec::new();
        let mut delta: HashMap<String, u64> = HashMap::new();
        for (k, e) in self.entries.iter() {
            let kl = k.to_ascii_lowercase();
            let exact = targets.iter().find(|(key_l, _)| key_l == &kl);
            if let Some((own_key, _)) = exact {
                doomed.push(k.clone());
                let covered_by_other = targets
                    .iter()
                    .any(|(other_key, prefix)| other_key != own_key && kl.starts_with(prefix));
                if covered_by_other {
                    continue;
                }
                for anc in e.path.ancestors().skip(1) {
                    if anc.as_os_str().is_empty() {
                        break;
                    }
                    let raw = Self::key(anc);
                    let ak = Self::key(Path::new(&raw));
                    *delta.entry(ak).or_default() += e.size;
                }
            } else if targets.iter().any(|(_, prefix)| kl.starts_with(prefix)) {
                doomed.push(k.clone());
            }
        }
        // 祖先本身也被整体删除时，其扣减由更上层完成，丢弃中间项
        let doomed_l: std::collections::HashSet<String> =
            doomed.iter().map(|k| k.to_ascii_lowercase()).collect();
        delta.retain(|ak, _| !doomed_l.contains(&ak.to_ascii_lowercase()));
        for k in doomed {
            if let Some(e) = self.entries.remove(&k) {
                if e.is_dir {
                    self.dir_count = self.dir_count.saturating_sub(1);
                } else {
                    self.file_count = self.file_count.saturating_sub(1);
                }
            }
        }
        for (pk, d) in delta {
            if let Some(e) = self.entries.get_mut(&pk) {
                if e.is_dir {
                    e.size = e.size.saturating_sub(d);
                }
                continue;
            }
            let pkl = pk.to_ascii_lowercase();
            if let Some((_, e)) = self
                .entries
                .iter_mut()
                .find(|(k, _)| k.to_ascii_lowercase() == pkl)
            {
                if e.is_dir {
                    e.size = e.size.saturating_sub(d);
                }
            }
        }
        self.rebuild_children();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Mtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

pub fn sort_entries(entries: &mut [&FsEntry], key: SortKey, dir: SortDir) {
    entries.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        let ord = match key {
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::Size => a.size.cmp(&b.size),
            SortKey::Mtime => a.mtime.cmp(&b.mtime),
        };
        match dir {
            SortDir::Asc => ord,
            SortDir::Desc => ord.reverse(),
        }
    });
}

pub fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", n, UNITS[i])
    } else {
        format!("{:.2} {}", v, UNITS[i])
    }
}

pub fn format_mtime(t: Option<SystemTime>) -> String {
    let Some(t) = t else {
        return "—".into();
    };
    let dt: chrono::DateTime<chrono::Local> = t.into();
    dt.format("%Y-%m-%d %H:%M").to_string()
}

pub fn format_delta(delta: i64) -> String {
    if delta >= 0 {
        format!("+{}", format_bytes(delta as u64))
    } else {
        format!("-{}", format_bytes((-delta) as u64))
    }
}

pub fn list_drives() -> Vec<PathBuf> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLogicalDrives() -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
    }
    let mask = unsafe { GetLogicalDrives() };
    let mut drives = Vec::new();
    for i in 0..26u8 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i) as char;
        let mut root = [
            u16::from(letter as u8),
            u16::from(b':'),
            u16::from(b'\\'),
            0,
        ];
        let dtype = unsafe { GetDriveTypeW(root.as_mut_ptr()) };
        // 2 removable, 3 fixed, 4 remote, 6 ram — 跳过空光驱(5 且可能无介质)
        if dtype == 2 || dtype == 3 || dtype == 4 || dtype == 6 {
            drives.push(PathBuf::from(format!("{letter}:\\")));
        } else if dtype == 5 {
            // 光驱有介质时偶尔可用，不主动 exists 探测以免卡住
        }
    }
    if drives.is_empty() {
        // 兜底
        for c in b'C'..=b'Z' {
            let p = PathBuf::from(format!("{}:\\", c as char));
            if p.exists() {
                drives.push(p);
            }
        }
    }
    drives
}

pub fn is_sensitive_path(path: &Path) -> bool {
    let s = path.to_string_lossy().to_lowercase();
    let needles = [
        "\\windows\\",
        ":\\windows",
        "\\program files",
        "\\program files (x86)",
        "\\programdata\\microsoft",
        "\\system32",
        "\\syswow64",
    ];
    needles.iter().any(|n| s.contains(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(path: &str, size: u64) -> FsEntry {
        FsEntry {
            path: PathBuf::from(path),
            name: "x".into(),
            is_dir: true,
            size,
            mtime: None,
            count_only: false,
        }
    }

    fn file(path: &str, size: u64) -> FsEntry {
        FsEntry {
            path: PathBuf::from(path),
            name: "x".into(),
            is_dir: false,
            size,
            mtime: None,
            count_only: false,
        }
    }

    fn idx(entries: Vec<(&str, FsEntry)>) -> ScanIndex {
        ScanIndex {
            entries: entries
                .into_iter()
                .map(|(k, e)| (ScanIndex::key(Path::new(k)), e))
                .collect(),
            ..ScanIndex::default()
        }
    }

    #[test]
    fn batch_removes_subtrees_and_deducts_parents_once() {
        let mut index = idx(vec![
            ("C:\\", dir("C:\\", 1000)),
            ("C:\\root", dir("C:\\root", 600)),
            ("C:\\root\\a", dir("C:\\root\\a", 300)),
            ("C:\\root\\a\\f1.txt", file("C:\\root\\a\\f1.txt", 300)),
            ("C:\\root\\b", dir("C:\\root\\b", 300)),
            ("C:\\root\\b\\f2.txt", file("C:\\root\\b\\f2.txt", 300)),
            ("C:\\root\\keep.txt", file("C:\\root\\keep.txt", 0)),
        ]);
        let targets = [
            PathBuf::from("C:\\root\\a"),
            PathBuf::from("C:\\root\\a\\f1.txt"), // 被上层覆盖，不应重复扣减
            PathBuf::from("C:\\root\\b"),
        ];
        index.remove_cascade_batch(&targets);
        assert!(index.get(Path::new("C:\\root\\a")).is_none());
        assert!(index.get(Path::new("C:\\root\\b")).is_none());
        assert!(index.get(Path::new("C:\\root\\a\\f1.txt")).is_none());
        assert_eq!(index.get(Path::new("C:\\root")).unwrap().size, 0);
        assert_eq!(index.get(Path::new("C:\\")).unwrap().size, 400);
        // children 不再指向已删除目录
        assert!(index.children_of(Path::new("C:\\root\\a")).is_empty());
    }

    #[test]
    fn batch_handles_case_insensitive_keys() {
        let mut index = idx(vec![
            ("C:\\Mix", dir("C:\\Mix", 500)),
            ("C:\\Mix\\f.txt", file("C:\\Mix\\f.txt", 500)),
        ]);
        index.remove_cascade_batch(&[PathBuf::from("c:\\mix")]);
        assert!(index.get(Path::new("C:\\Mix")).is_none());
        assert!(index.get(Path::new("C:\\Mix\\f.txt")).is_none());
    }
}
