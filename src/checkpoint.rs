//! 扫盘断点：取消时保存剩余目录 + 已扫索引，重启后可真续扫

use crate::model::ScanIndex;
use crate::persistence;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanCheckpoint {
    pub root: String,
    pub remaining: Vec<String>,
    pub visited: u64,
    pub skipped: u64,
    pub bytes_seen: u64,
}

impl ScanCheckpoint {
    pub fn path() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("disk-janitor").join("scan-checkpoint.json")
    }

    pub fn index_path() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("disk-janitor").join("scan-index.json")
    }

    pub fn save(&self) -> Result<(), String> {
        save_versioned(&Self::path(), self)
    }

    pub fn save_with_index(&self, index: &ScanIndex) -> Result<(), String> {
        // 索引先落盘，checkpoint 后落盘作为提交标记；ScanIndex.children 本身 serde(skip)，
        // 直接借用序列化即可避免取消时再 clone 一份完整索引。
        save_versioned(&Self::index_path(), index)?;
        self.save()
    }

    pub fn load() -> Option<Self> {
        load_versioned_or_legacy(&Self::path())
    }

    pub fn load_index() -> Option<ScanIndex> {
        let mut idx: ScanIndex = load_versioned_or_legacy(&Self::index_path())?;
        idx.rebuild_children();
        Some(idx)
    }

    pub fn clear() {
        let _ = std::fs::remove_file(Self::path());
        let _ = std::fs::remove_file(Self::index_path());
    }

    pub fn from_cancel(
        root: &std::path::Path,
        remaining: &[PathBuf],
        _index: &ScanIndex,
        visited: u64,
        skipped: u64,
        bytes_seen: u64,
    ) -> Self {
        Self {
            root: root.display().to_string(),
            remaining: remaining
                .iter()
                .map(|p| p.display().to_string())
                .take(50_000)
                .collect(),
            visited,
            skipped,
            bytes_seen,
        }
    }
}

fn save_versioned<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), String> {
    persistence::save_json(path, value, false)
}

fn load_versioned_or_legacy<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return None,
    };
    match decode_versioned_or_legacy(&bytes) {
        Ok(value) => Some(value),
        Err(_) => {
            preserve_corrupt(path);
            None
        }
    }
}

fn decode_versioned_or_legacy<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    persistence::decode_json(bytes)
}

fn preserve_corrupt(path: &Path) {
    persistence::preserve_corrupt(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkpoint() -> ScanCheckpoint {
        ScanCheckpoint {
            root: r"C:\data".into(),
            remaining: vec![r"C:\data\pending".into()],
            visited: 12,
            skipped: 3,
            bytes_seen: 456,
        }
    }

    #[test]
    fn versioned_atomic_round_trip_uses_temp_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("checkpoint.json");
        save_versioned(&path, &checkpoint()).unwrap();

        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains("\"schemaVersion\":1"));
        let loaded: ScanCheckpoint = load_versioned_or_legacy(&path).unwrap();
        assert_eq!(loaded.root, r"C:\data");
        assert_eq!(loaded.remaining.len(), 1);

        let mut replacement = checkpoint();
        replacement.visited = 99;
        save_versioned(&path, &replacement).unwrap();
        let replaced: ScanCheckpoint = load_versioned_or_legacy(&path).unwrap();
        assert_eq!(replaced.visited, 99);
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "原子写入不应遗留临时文件"
        );
    }

    #[test]
    fn loads_legacy_json_from_temp_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        std::fs::write(&path, serde_json::to_vec(&checkpoint()).unwrap()).unwrap();

        let loaded: ScanCheckpoint = load_versioned_or_legacy(&path).unwrap();
        assert_eq!(loaded.visited, 12);
        assert!(path.exists());
    }

    #[test]
    fn corrupt_json_is_preserved_in_temp_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.json");
        std::fs::write(&path, b"{not-json").unwrap();

        assert!(load_versioned_or_legacy::<ScanCheckpoint>(&path).is_none());
        assert!(!path.exists());
        assert!(dir.path().join("broken.json.corrupt").exists());
    }
}
