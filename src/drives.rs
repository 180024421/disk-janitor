//! 盘符容量总览（无需全盘扫描）

use crate::model::format_bytes;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DriveInfo {
    pub root: PathBuf,
    pub label: String,
    pub total: u64,
    pub free: u64,
    pub kind: String,
}

impl DriveInfo {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.free)
    }

    pub fn used_ratio(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.used() as f32 / self.total as f32
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "{} 已用 {} / {}（可用 {}）",
            self.label,
            format_bytes(self.used()),
            format_bytes(self.total),
            format_bytes(self.free)
        )
    }
}

pub fn list_drive_infos() -> Vec<DriveInfo> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLogicalDrives() -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
        fn GetDiskFreeSpaceExW(
            root: *const u16,
            free_bytes_available: *mut u64,
            total_bytes: *mut u64,
            total_free_bytes: *mut u64,
        ) -> i32;
        fn GetVolumeInformationW(
            root: *const u16,
            name: *mut u16,
            name_len: u32,
            serial: *mut u32,
            max_comp: *mut u32,
            flags: *mut u32,
            fs: *mut u16,
            fs_len: u32,
        ) -> i32;
    }

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;
    const DRIVE_RAMDISK: u32 = 6;

    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u8 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i) as char;
        let mut root_w = [
            u16::from(letter as u8),
            u16::from(b':'),
            u16::from(b'\\'),
            0,
        ];
        let dtype = unsafe { GetDriveTypeW(root_w.as_mut_ptr()) };
        // 跳过空光驱 / 未知，避免卡住；网络盘也尝试读容量，失败则跳过
        if dtype == 1 {
            continue;
        }
        let kind = match dtype {
            DRIVE_REMOVABLE => "可移动",
            DRIVE_FIXED => "本地磁盘",
            DRIVE_REMOTE => "网络",
            DRIVE_CDROM => "光驱",
            DRIVE_RAMDISK => "RAM",
            _ => "其他",
        };
        let mut avail = 0u64;
        let mut total = 0u64;
        let mut free = 0u64;
        let ok =
            unsafe { GetDiskFreeSpaceExW(root_w.as_mut_ptr(), &mut avail, &mut total, &mut free) };
        if ok == 0 || total == 0 {
            continue;
        }
        let mut name_buf = [0u16; 64];
        let mut serial = 0u32;
        let mut max_comp = 0u32;
        let mut flags = 0u32;
        let mut fs_buf = [0u16; 32];
        let _ = unsafe {
            GetVolumeInformationW(
                root_w.as_mut_ptr(),
                name_buf.as_mut_ptr(),
                name_buf.len() as u32,
                &mut serial,
                &mut max_comp,
                &mut flags,
                fs_buf.as_mut_ptr(),
                fs_buf.len() as u32,
            )
        };
        let vol = String::from_utf16_lossy(
            &name_buf[..name_buf.iter().position(|&c| c == 0).unwrap_or(0)],
        );
        let label = if vol.trim().is_empty() {
            format!("{letter}:")
        } else {
            format!("{letter}: ({vol})")
        };
        out.push(DriveInfo {
            root: PathBuf::from(format!("{letter}:\\")),
            label,
            total,
            free: avail,
            kind: kind.into(),
        });
    }
    out
}
