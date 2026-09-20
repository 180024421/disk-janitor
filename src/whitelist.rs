//! 残留路径白名单（误报后不再提示）

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LeftoverWhitelist {
    #[serde(default)]
    pub paths: Vec<String>,
}

impl LeftoverWhitelist {
    pub fn path() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("disk-janitor").join("leftover-whitelist.json")
    }

    pub fn load() -> Self {
        let mut wl: Self = crate::persistence::load_json(&Self::path()).unwrap_or_default();
        // 手工编辑/旧版本写入的条目可能未规范化，入口处统一归一并去重。
        let mut seen = HashSet::new();
        wl.paths.retain(|p| seen.insert(norm_key(Path::new(p))));
        wl.paths = wl
            .paths
            .iter()
            .map(|p| norm_key(Path::new(p)))
            .filter(|k| !k.is_empty())
            .collect();
        wl.paths.sort();
        wl
    }

    pub fn save(&self) -> Result<(), String> {
        crate::persistence::save_json(&Self::path(), self, true)
    }

    pub fn set(&self) -> HashSet<String> {
        self.paths.iter().map(|p| norm_key(Path::new(p))).collect()
    }

    /// 命中即白名单：完全相等，或位于某白名单目录之下（前缀匹配）。
    pub fn contains(&self, path: &Path) -> bool {
        let key = norm_key(path);
        if key.is_empty() {
            return false;
        }
        self.paths.iter().any(|stored| {
            // 空串 starts_with 恒真，会误匹配一切路径，必须排除。
            let p = norm_key(Path::new(stored));
            !p.is_empty()
                && (key == p
                    || (key.len() > p.len()
                        && key.starts_with(p.as_str())
                        && key.as_bytes()[p.len()] == b'\\'))
        })
    }

    pub fn add(&mut self, path: &Path) -> bool {
        let key = norm_key(path);
        if self.paths.iter().any(|p| p == &key) {
            return false;
        }
        self.paths.push(key);
        self.paths.sort();
        true
    }

    pub fn remove(&mut self, path: &Path) -> bool {
        let key = norm_key(path);
        let before = self.paths.len();
        self.paths.retain(|p| p != &key);
        self.paths.len() != before
    }
}

pub fn filter_whitelisted(
    hits: Vec<crate::leftovers::LeftoverHit>,
    wl: &LeftoverWhitelist,
) -> Vec<crate::leftovers::LeftoverHit> {
    hits.into_iter().filter(|h| !wl.contains(&h.path)).collect()
}

fn norm_key(path: &Path) -> String {
    path.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
        .replace('/', "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_prefix_matches_children() {
        let mut wl = LeftoverWhitelist::default();
        assert!(wl.add(Path::new(r"D:\Apps\Foo")));
        assert!(wl.contains(Path::new(r"d:\apps\foo")));
        assert!(wl.contains(Path::new(r"D:\Apps\Foo\bar\baz.txt")));
        assert!(!wl.contains(Path::new(r"D:\Apps\FooBar")));
        assert!(!wl.contains(Path::new(r"D:\Apps\Other")));
    }

    #[test]
    fn tolerates_unnormalized_and_empty_stored_entries() {
        // 模拟手编 JSON：正斜杠、大小写、尾部反斜杠与空串。
        let wl = LeftoverWhitelist {
            paths: vec![
                "D:\\Apps\\Foo\\".into(),
                "d:/apps/bar".into(),
                String::new(),
            ],
        };
        assert!(wl.contains(Path::new(r"D:\APPS\FOO\x")));
        assert!(wl.contains(Path::new(r"D:\Apps\Bar\y")));
        // 空串不得匹配任意路径
        let only_empty = LeftoverWhitelist {
            paths: vec![String::new()],
        };
        assert!(!only_empty.contains(Path::new(r"\\server\share")));
        assert!(!only_empty.contains(Path::new(r"C:\Anything")));
    }
}
