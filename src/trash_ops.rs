//! 删除：优先进回收站，失败则换方式，再失败才直接删

use crate::model::is_sensitive_path;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Default)]
pub struct TrashResult {
    pub ok: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
    /// 走了「直接删除」而非回收站的数量
    pub permanent: u64,
    pub skipped_locked: u64,
}

/// 浏览页删除：回收站 → PowerShell 回收站 → 直接删除
pub fn move_to_trash(paths: &[PathBuf]) -> TrashResult {
    let mut res = TrashResult::default();
    for p in paths {
        if !p.exists() {
            // 已不存在：视为已处理成功，便于刷新索引
            res.ok.push(p.clone());
            continue;
        }
        match delete_with_fallback(p) {
            DeleteOutcome::Recycled => res.ok.push(p.clone()),
            DeleteOutcome::Permanent => {
                res.ok.push(p.clone());
                res.permanent += 1;
            }
            DeleteOutcome::Locked => {
                res.skipped_locked += 1;
                res.failed.push((
                    p.clone(),
                    "文件正在使用中，请先关闭 WSL/相关程序后再删".into(),
                ));
            }
            DeleteOutcome::Failed(msg) => res.failed.push((p.clone(), msg)),
        }
    }
    res
}

enum DeleteOutcome {
    Recycled,
    Permanent,
    Locked,
    Failed(String),
}

fn delete_with_fallback(p: &Path) -> DeleteOutcome {
    // 1) trash crate
    if trash::delete(p).is_ok() {
        return DeleteOutcome::Recycled;
    }
    // 2) VisualBasic 回收站（不依赖 canonicalize，对 {guid} 路径更稳）
    if trash_via_vb(p).is_ok() {
        return DeleteOutcome::Recycled;
    }
    // 3) 直接删除
    let r = if p.is_dir() {
        fs::remove_dir_all(p)
    } else {
        fs::remove_file(p)
    };
    match r {
        Ok(()) => DeleteOutcome::Permanent,
        Err(e) => {
            let msg = e.to_string();
            if is_locked_msg(&msg) {
                DeleteOutcome::Locked
            } else {
                DeleteOutcome::Failed(format!("回收站与直接删除均失败: {e}"))
            }
        }
    }
}

fn is_locked_msg(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("being used")
        || m.contains("os error 32")
        || m.contains("os error 5")
        || m.contains("access is denied")
        || m.contains("拒绝访问")
        || m.contains("cannot access")
}

fn trash_via_vb(path: &Path) -> Result<(), String> {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy().replace('\'', "''");
    let is_dir = abs.is_dir();
    let method = if is_dir {
        format!(
            "[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory('{s}', 'OnlyErrorDialogs', 'SendToRecycleBin')"
        )
    } else {
        format!(
            "[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile('{s}', 'OnlyErrorDialogs', 'SendToRecycleBin')"
        )
    };
    let script = format!("Add-Type -AssemblyName Microsoft.VisualBasic; {method}");
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() && !abs.exists() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(if err.trim().is_empty() {
            "PowerShell 回收站失败".into()
        } else {
            err.trim().to_string()
        })
    }
}

/// 清理垃圾路径：目录只删其下内容（不删 Temp 根目录本身）
pub fn clean_junk_paths(paths: &[PathBuf]) -> TrashResult {
    let mut res = TrashResult::default();
    let mut seen = HashSet::new();

    for raw in paths {
        let p = raw.to_path_buf();
        let key = p.to_string_lossy().to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        if !p.exists() {
            continue;
        }
        if p.is_file() {
            apply_outcome(delete_with_fallback(&p), &p, &mut res);
            continue;
        }
        if p.is_dir() {
            clean_dir_contents(&p, &mut res, 0);
        }
    }
    res
}

fn apply_outcome(o: DeleteOutcome, p: &Path, res: &mut TrashResult) {
    match o {
        DeleteOutcome::Recycled => res.ok.push(p.to_path_buf()),
        DeleteOutcome::Permanent => {
            res.ok.push(p.to_path_buf());
            res.permanent += 1;
        }
        DeleteOutcome::Locked => {
            res.skipped_locked += 1;
            res.failed
                .push((p.to_path_buf(), "占用中，已跳过".into()));
        }
        DeleteOutcome::Failed(msg) => res.failed.push((p.to_path_buf(), msg)),
    }
}

fn clean_dir_contents(dir: &Path, res: &mut TrashResult, depth: u32) {
    if depth > 2 {
        apply_outcome(delete_with_fallback(dir), dir, res);
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        res.failed
            .push((dir.to_path_buf(), "无法读取目录".into()));
        return;
    };
    for ent in rd.flatten() {
        let child = ent.path();
        let Ok(ft) = ent.file_type() else {
            continue;
        };
        if ft.is_dir() {
            let outcome = delete_with_fallback(&child);
            let still_there = child.exists();
            match &outcome {
                DeleteOutcome::Recycled | DeleteOutcome::Permanent => {
                    apply_outcome(outcome, &child, res);
                }
                DeleteOutcome::Locked | DeleteOutcome::Failed(_) if still_there && depth < 2 => {
                    // 整目录删不掉：深入清内容，不把整目录记成失败刷屏
                    clean_dir_contents(&child, res, depth + 1);
                }
                _ => apply_outcome(outcome, &child, res),
            }
        } else {
            apply_outcome(delete_with_fallback(&child), &child, res);
        }
    }
}

pub fn any_sensitive(paths: &[PathBuf]) -> bool {
    paths.iter().any(|p| is_sensitive_path(Path::new(p)))
}

pub fn format_trash_errors(res: &TrashResult, limit: usize) -> String {
    let mut lines: Vec<String> = res
        .failed
        .iter()
        .take(limit)
        .map(|(p, e)| format!("{}: {}", p.display(), e))
        .collect();
    if res.failed.len() > limit {
        lines.push(format!("…另有 {} 条失败", res.failed.len() - limit));
    }
    if res.skipped_locked > 0 {
        lines.push(format!(
            "跳过占用中约 {} 个（关闭 WSL/程序后可再删）",
            res.skipped_locked
        ));
    }
    if res.permanent > 0 {
        lines.push(format!(
            "其中 {} 项无法进回收站，已直接删除",
            res.permanent
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn trash_moves_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("disk-janitor-trash-test.txt");
        fs::write(&f, b"hello-trash").unwrap();
        assert!(f.exists());
        let res = move_to_trash(&[f.clone()]);
        assert!(res.failed.is_empty(), "{:?}", res.failed);
        assert!(!f.exists());
    }

    #[test]
    fn clean_junk_contents_keeps_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("TempRoot");
        fs::create_dir(&root).unwrap();
        let f = root.join("a.txt");
        fs::write(&f, b"x").unwrap();
        let res = clean_junk_paths(&[root.clone()]);
        assert!(root.exists(), "Temp 根目录应保留");
        assert!(!f.exists());
        assert!(res.failed.is_empty(), "{:?}", res.failed);
    }

    #[test]
    fn braces_path_file_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("{abc-def-123}");
        fs::create_dir(&nested).unwrap();
        let f = nested.join("t.txt");
        fs::write(&f, b"x").unwrap();
        let res = move_to_trash(&[f.clone()]);
        assert!(res.failed.is_empty(), "{:?}", res.failed);
        assert!(!f.exists());
    }
}
