//! 扫盘断点：取消时保存剩余目录 + 已扫索引，重启后可真续扫

use crate::model::ScanIndex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
        let p = Self::path();
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let s = serde_json::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(p, s).map_err(|e| e.to_string())
    }

    pub fn save_with_index(&self, index: &ScanIndex) -> Result<(), String> {
        self.save()?;
        let mut idx = index.clone();
        idx.children.clear(); // 加载后再 rebuild
        let s = serde_json::to_string(&idx).map_err(|e| e.to_string())?;
        std::fs::write(Self::index_path(), s).map_err(|e| e.to_string())
    }

    pub fn load() -> Option<Self> {
        let s = std::fs::read_to_string(Self::path()).ok()?;
        serde_json::from_str(&s).ok()
    }

    pub fn load_index() -> Option<ScanIndex> {
        let s = std::fs::read_to_string(Self::index_path()).ok()?;
        let mut idx: ScanIndex = serde_json::from_str(&s).ok()?;
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
