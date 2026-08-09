//! 文件系统扫描结果模型

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct FsEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    /// 文件：自身大小；目录：下属文件合计
    pub size: u64,
    pub mtime: Option<SystemTime>,
}

#[derive(Debug, Clone, Default)]
pub struct ScanIndex {
    pub root: PathBuf,
    /// 规范化路径字符串 -> 条目
    pub entries: HashMap<String, FsEntry>,
    /// 父路径(小写) -> 子条目 key 列表（避免每次 O(全表) 找子项）
    pub children: HashMap<String, Vec<String>>,
    pub skipped: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub errors: Vec<String>,
    /// 扫描是否仍在进行（增量快照时为 true）
    pub partial: bool,
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
        self.entries.get(&Self::key(path))
    }

    /// 重建父子索引（扫盘结束 / 增量快照后调用）
    pub fn rebuild_children(&mut self) {
        self.children.clear();
        // 预估容量，减少 rehash
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

    /// 列出某一目录下的直接子项（O(子项数)，不是 O(全盘)）
    pub fn children_of(&self, dir: &Path) -> Vec<&FsEntry> {
        let parent = Self::key_norm(dir);
        let Some(keys) = self.children.get(&parent) else {
            return Vec::new();
        };
        keys.iter()
            .filter_map(|k| self.entries.get(k))
            .collect()
    }

    /// 仅子目录（侧边树）；可按大小截断
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

    /// 空目录（占用为 0 的文件夹），按路径排序后截断
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
        let key = Self::key(path);
        let size = self.entries.get(&key).map(|e| e.size).unwrap_or(0);
        let prefix = if key.ends_with('\\') {
            key.clone()
        } else {
            format!("{}\\", key)
        };
        let doomed: Vec<String> = self
            .entries
            .keys()
            .filter(|k| {
                let kk = k.as_str();
                kk.eq_ignore_ascii_case(&key)
                    || kk
                        .to_ascii_lowercase()
                        .starts_with(&prefix.to_ascii_lowercase())
            })
            .cloned()
            .collect();
        for k in doomed {
            if let Some(e) = self.entries.remove(&k) {
                if e.is_dir {
                    self.dir_count = self.dir_count.saturating_sub(1);
                } else {
                    self.file_count = self.file_count.saturating_sub(1);
                }
            }
        }
        for anc in path.ancestors().skip(1) {
            if anc.as_os_str().is_empty() {
                break;
            }
            let ak = Self::key(anc);
            if let Some(e) = self.entries.get_mut(&ak) {
                if e.is_dir {
                    e.size = e.size.saturating_sub(size);
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
    let Ok(dur) = t.duration_since(SystemTime::UNIX_EPOCH) else {
        return "—".into();
    };
    let secs = dur.as_secs() as i64;
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "—".into())
}

pub fn list_drives() -> Vec<PathBuf> {
    let mut drives = Vec::new();
    for c in b'A'..=b'Z' {
        let p = PathBuf::from(format!("{}:\\", c as char));
        if p.exists() {
            drives.push(p);
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
