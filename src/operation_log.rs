//! 本地破坏性操作日志，便于解释清理结果和恢复排查。

use chrono::Local;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct OperationRecord<'a> {
    pub timestamp: String,
    pub action: &'a str,
    pub target: String,
    pub outcome: &'a str,
    pub detail: &'a str,
}

pub fn append(action: &str, target: &Path, outcome: &str, detail: &str) {
    let record = OperationRecord {
        timestamp: Local::now().to_rfc3339(),
        action,
        target: target.display().to_string(),
        outcome,
        detail,
    };
    let Ok(mut line) = serde_json::to_string(&record) else {
        return;
    };
    line.push('\n');
    let path = history_path();
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    }
}

pub fn history_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("disk-janitor").join("operation-history.jsonl")
}

pub fn read_recent(limit: usize) -> Vec<String> {
    let Ok(content) = fs::read_to_string(history_path()) else {
        return Vec::new();
    };
    let mut lines: Vec<String> = content
        .lines()
        .rev()
        .take(limit)
        .map(|line| line.to_string())
        .collect();
    lines.reverse();
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_has_stable_location() {
        assert!(history_path()
            .to_string_lossy()
            .contains("operation-history.jsonl"));
    }
}
