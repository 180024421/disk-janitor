//! 删除：优先进回收站；可选允许永久删除兜底；安全粉碎；回收站清空

use crate::model::is_sensitive_path;
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Default)]
pub struct TrashResult {
    pub ok: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
    /// 走了「直接删除」而非回收站的数量
    pub permanent: u64,
    pub skipped_locked: u64,
    /// 安全粉碎的文件数
    pub shredded: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct DeleteOptions {
    /// 回收站失败时是否允许直接删除
    pub allow_permanent: bool,
    /// 安全粉碎（仅文件：覆写后永久删除，不可进回收站）
    pub shred: bool,
}

impl Default for DeleteOptions {
    fn default() -> Self {
        Self {
            allow_permanent: false,
            shred: false,
        }
    }
}

pub fn move_to_trash(paths: &[PathBuf]) -> TrashResult {
    move_to_trash_with(paths, DeleteOptions::default())
}

pub fn move_to_trash_with(paths: &[PathBuf], opts: DeleteOptions) -> TrashResult {
    let mut res = TrashResult::default();
    for p in paths {
        if !p.exists() {
            res.ok.push(p.clone());
            continue;
        }
        if opts.shred {
            if p.is_file() {
                match shred_file(p, 3) {
                    Ok(()) => {
                        res.ok.push(p.clone());
                        res.shredded += 1;
                        res.permanent += 1;
                    }
                    Err(e) => res.failed.push((p.clone(), e)),
                }
            } else {
                res.failed.push((
                    p.clone(),
                    "安全粉碎仅支持文件（目录请先展开或改用不粉碎删除）".into(),
                ));
            }
            continue;
        }
        match delete_with_fallback(p, opts.allow_permanent) {
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

/// 覆写文件内容（零 / 伪随机）后永久删除。
pub fn shred_file(path: &Path, passes: u8) -> Result<(), String> {
    if !path.is_file() {
        return Err("不是普通文件".into());
    }
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    let len = meta.len();
    let passes = passes.max(1);
    {
        let mut f = OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 64 * 1024];
        for pass in 0..passes {
            f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            if pass % 2 == 0 {
                buf.fill(0);
            } else {
                let mut x = (len ^ (pass as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
                    .wrapping_add(0xA5A5_5A5A);
                for b in buf.iter_mut() {
                    x = x
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1);
                    *b = (x >> 33) as u8;
                }
            }
            let mut remaining = len;
            while remaining > 0 {
                let n = remaining.min(buf.len() as u64) as usize;
                f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                remaining -= n as u64;
            }
            f.flush().map_err(|e| e.to_string())?;
        }
    }
    fs::remove_file(path).map_err(|e| e.to_string())?;
    Ok(())
}

enum DeleteOutcome {
    Recycled,
    Permanent,
    Locked,
    Failed(String),
}

fn delete_with_fallback(p: &Path, allow_permanent: bool) -> DeleteOutcome {
    if trash::delete(p).is_ok() {
        return DeleteOutcome::Recycled;
    }
    if trash_via_vb(p).is_ok() {
        return DeleteOutcome::Recycled;
    }
    if !allow_permanent {
        return DeleteOutcome::Failed(
            "无法移入回收站（已跳过永久删除；可在确认框勾选「允许直接删除」）".into(),
        );
    }
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

pub fn clean_junk_paths(paths: &[PathBuf]) -> TrashResult {
    clean_junk_paths_with(
        paths,
        DeleteOptions {
            allow_permanent: true,
            shred: false,
        },
    )
}

pub fn clean_junk_paths_with(paths: &[PathBuf], opts: DeleteOptions) -> TrashResult {
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
            if opts.shred {
                match shred_file(&p, 3) {
                    Ok(()) => {
                        res.ok.push(p.clone());
                        res.shredded += 1;
                        res.permanent += 1;
                    }
                    Err(e) => res.failed.push((p, e)),
                }
            } else {
                apply_outcome(delete_with_fallback(&p, opts.allow_permanent), &p, &mut res);
            }
            continue;
        }
        if p.is_dir() {
            clean_dir_contents(&p, &mut res, 0, opts.allow_permanent);
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

fn clean_dir_contents(dir: &Path, res: &mut TrashResult, depth: u32, allow_permanent: bool) {
    if depth > 2 {
        apply_outcome(delete_with_fallback(dir, allow_permanent), dir, res);
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
            let outcome = delete_with_fallback(&child, allow_permanent);
            let still_there = child.exists();
            match &outcome {
                DeleteOutcome::Recycled | DeleteOutcome::Permanent => {
                    apply_outcome(outcome, &child, res);
                }
                DeleteOutcome::Locked | DeleteOutcome::Failed(_) if still_there && depth < 2 => {
                    clean_dir_contents(&child, res, depth + 1, allow_permanent);
                }
                _ => apply_outcome(outcome, &child, res),
            }
        } else {
            apply_outcome(delete_with_fallback(&child, allow_permanent), &child, res);
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
    if res.shredded > 0 {
        lines.push(format!("已安全粉碎 {} 个文件", res.shredded));
    }
    if res.permanent > 0 {
        lines.push(format!(
            "其中 {} 项无法进回收站，已直接删除",
            res.permanent
        ));
    }
    lines.join("\n")
}

/// 估算回收站占用（用户 SID 下 $Recycle.Bin）
pub fn recycle_bin_size() -> Result<(u64, u64), String> {
    let mut total = 0u64;
    let mut files = 0u64;
    for letter in b'A'..=b'Z' {
        let bin = PathBuf::from(format!("{}:\\$Recycle.Bin", letter as char));
        if !bin.exists() {
            continue;
        }
        let Ok(rd) = fs::read_dir(&bin) else {
            continue;
        };
        for ent in rd.flatten() {
            let p = ent.path();
            if p.is_dir() {
                let (sz, n) = crate::scan::quick_dir_size(
                    &p,
                    &std::sync::atomic::AtomicBool::new(false),
                    50_000,
                );
                total += sz;
                files += n;
            }
        }
    }
    Ok((total, files))
}

pub fn empty_recycle_bin() -> Result<String, String> {
    let script = r#"
$shell = New-Object -ComObject Shell.Application
$rb = $shell.NameSpace(0xA)
if ($null -eq $rb) { throw '无法打开回收站' }
$items = @($rb.Items())
$count = $items.Count
foreach ($i in $items) { Remove-Item -LiteralPath $i.Path -Recurse -Force -ErrorAction SilentlyContinue }
"已请求清空回收站（约 $count 项）"
"#;
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        let msg = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(if msg.is_empty() {
            "已请求清空回收站".into()
        } else {
            msg
        })
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
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
    fn no_permanent_without_flag() {
        let opts = DeleteOptions::default();
        assert!(!opts.allow_permanent);
        assert!(!opts.shred);
    }

    #[test]
    fn shred_overwrites_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("secret.bin");
        fs::write(&f, b"TOPSECRETDATA123").unwrap();
        shred_file(&f, 2).unwrap();
        assert!(!f.exists());
    }
}
