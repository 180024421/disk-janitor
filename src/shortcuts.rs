//! 失效快捷方式：只扫桌面 .lnk；自写轻量解析（不用 lnk 库）；硬超时

use crate::software::expand_env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const MAX_LNKS: usize = 200;
const SCAN_BUDGET: Duration = Duration::from_secs(3);

#[derive(Debug, Clone)]
pub struct BrokenShortcut {
    pub path: PathBuf,
    pub target: String,
    pub location: String,
    pub selected: bool,
}

pub fn scan_broken_shortcuts(cancel: &AtomicBool) -> Vec<BrokenShortcut> {
    let started = Instant::now();
    let mut out = Vec::new();
    let mut checked = 0usize;
    let local = local_fixed_letters();

    // 只扫桌面（用户 + 公共），不进开始菜单，避免又慢又容易卡
    for root in shortcut_roots() {
        if stop(cancel, started) || checked >= MAX_LNKS {
            break;
        }
        scan_dir_flat(&root, cancel, started, &local, &mut out, &mut checked);
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn stop(cancel: &AtomicBool, started: Instant) -> bool {
    cancel.load(Ordering::Relaxed) || started.elapsed() >= SCAN_BUDGET
}

fn shortcut_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(up) = std::env::var("USERPROFILE") {
        roots.push(PathBuf::from(up).join("Desktop"));
    }
    roots.push(PathBuf::from(r"C:\Users\Public\Desktop"));
    roots
}

/// 只扫该目录下一层的 .lnk，不递归
fn scan_dir_flat(
    dir: &Path,
    cancel: &AtomicBool,
    started: Instant,
    local: &[u8],
    out: &mut Vec<BrokenShortcut>,
    checked: &mut usize,
) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let location = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Desktop".into());

    for ent in rd.flatten() {
        if stop(cancel, started) || *checked >= MAX_LNKS {
            return;
        }
        let path = ent.path();
        if !path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("lnk"))
            .unwrap_or(false)
        {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if !meta.is_file() || meta.len() > 32 * 1024 {
            continue;
        }
        *checked += 1;
        let Some(target) = read_lnk_local_path(&path) else {
            continue;
        };
        let t = target.trim().to_string();
        if t.is_empty() || !should_check_target(&t, local) {
            continue;
        }
        if !Path::new(&t).exists() {
            out.push(BrokenShortcut {
                path,
                target: t,
                location: location.clone(),
                selected: true,
            });
        }
    }
}

fn should_check_target(t: &str, local: &[u8]) -> bool {
    let lower = t.to_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("shell:")
        || lower.starts_with("ms-")
    {
        return false;
    }
    if t.starts_with("\\\\") {
        return false;
    }
    let b = t.as_bytes();
    if b.len() < 3 || b[1] != b':' {
        return false;
    }
    let letter = b[0].to_ascii_uppercase();
    local.contains(&letter)
}

fn local_fixed_letters() -> Vec<u8> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLogicalDrives() -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
    }
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_CDROM: u32 = 5;
    const DRIVE_RAMDISK: u32 = 6;

    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u8 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = b'A' + i;
        let mut root = [
            u16::from(letter),
            u16::from(b':'),
            u16::from(b'\\'),
            0,
        ];
        let dtype = unsafe { GetDriveTypeW(root.as_mut_ptr()) };
        if matches!(
            dtype,
            DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_CDROM | DRIVE_RAMDISK
        ) {
            out.push(letter);
        }
    }
    out
}

/// 从 .lnk 二进制里尽量读出本地目标路径；失败返回 None，绝不 panic
fn read_lnk_local_path(path: &Path) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 0x4C + 4 {
        return None;
    }
    // ShellLinkHeader
    if data.get(0..4) != Some(&[0x4C, 0x00, 0x00, 0x00]) {
        return None;
    }
    let flags = u32::from_le_bytes(data.get(0x14..0x18)?.try_into().ok()?);
    let mut cursor = 0x4Cu32;

    // HasLinkTargetIDList
    if flags & 0x01 != 0 {
        let id_len = u16::from_le_bytes(data.get(cursor as usize..cursor as usize + 2)?.try_into().ok()?)
            as u32;
        cursor = cursor.checked_add(2)?.checked_add(id_len)?;
    }

    // HasLinkInfo
    if flags & 0x02 == 0 {
        return None;
    }
    if cursor as usize + 28 > data.len() {
        return None;
    }
    let link_info = &data[cursor as usize..];
    let link_info_size = u32::from_le_bytes(link_info.get(0..4)?.try_into().ok()?) as usize;
    if link_info_size < 28 || link_info_size > link_info.len() {
        return None;
    }
    let header_size = u32::from_le_bytes(link_info.get(4..8)?.try_into().ok()?) as usize;
    let info_flags = u32::from_le_bytes(link_info.get(8..12)?.try_into().ok()?);
    // VolumeIDAndLocalBasePath
    if info_flags & 0x01 == 0 {
        return None;
    }
    let local_base_path_offset =
        u32::from_le_bytes(link_info.get(16..20)?.try_into().ok()?) as usize;
    if local_base_path_offset >= link_info_size {
        return None;
    }

    // 优先 Unicode（header >= 0x24 时可能有）
    if header_size >= 0x24 && link_info_size >= 36 {
        let uni_off = u32::from_le_bytes(link_info.get(28..32)?.try_into().ok()?) as usize;
        if uni_off > 0 && uni_off < link_info_size {
            if let Some(s) = read_nul_utf16(&link_info[uni_off..link_info_size]) {
                let s = expand_env(&s);
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }

    let s = read_nul_ansi(&link_info[local_base_path_offset..link_info_size])?;
    let s = expand_env(&s);
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn read_nul_ansi(buf: &[u8]) -> Option<String> {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    if end == 0 {
        return None;
    }
    // Windows 本地路径多为 ASCII / 系统 ANSI；用 lossy 足够判断是否存在
    Some(String::from_utf8_lossy(&buf[..end]).into_owned())
}

fn read_nul_utf16(buf: &[u8]) -> Option<String> {
    if buf.len() < 2 {
        return None;
    }
    let mut u16s = Vec::new();
    let mut i = 0;
    while i + 1 < buf.len() {
        let c = u16::from_le_bytes([buf[i], buf[i + 1]]);
        if c == 0 {
            break;
        }
        u16s.push(c);
        i += 2;
        if u16s.len() > 512 {
            break;
        }
    }
    if u16s.is_empty() {
        return None;
    }
    String::from_utf16(&u16s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_non_empty() {
        assert!(!shortcut_roots().is_empty());
    }

    #[test]
    fn skip_unc() {
        let local = local_fixed_letters();
        assert!(!should_check_target(r"\\server\share\a.exe", &local));
    }

    #[test]
    fn scan_finishes_quickly() {
        let cancel = AtomicBool::new(false);
        let t0 = Instant::now();
        let _ = scan_broken_shortcuts(&cancel);
        assert!(t0.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn rejects_bad_header() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("bad.lnk");
        std::fs::write(&p, b"not a lnk").unwrap();
        assert!(read_lnk_local_path(&p).is_none());
    }
}
