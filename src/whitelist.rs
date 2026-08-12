//! 残留路径白名单（误报后不再提示）

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
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
        let p = Self::path();
        if let Ok(s) = fs::read_to_string(&p) {
            serde_json::from_str(&s).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::path();
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(p, s).map_err(|e| e.to_string())
    }

    pub fn set(&self) -> HashSet<String> {
        self.paths.iter().map(|p| norm_key(Path::new(p))).collect()
    }

    /// 命中即白名单：完全相等，或位于某白名单目录之下（前缀匹配）。
    pub fn contains(&self, path: &Path) -> bool {
        let key = norm_key(path);
        self.paths.iter().any(|p| {
            key == *p
                || (key.len() > p.len()
                    && key.starts_with(p.as_str())
                    && key.as_bytes()[p.len()] == b'\\')
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
    hits.into_iter()
        .filter(|h| !wl.contains(&h.path))
        .collect()
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
}
