//! 按扩展名粗分文件类型（大文件筛选）

use crate::model::FsEntry;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    All,
    Video,
    Audio,
    Image,
    Archive,
    Installer,
    Document,
    Other,
}

impl FileKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "全部",
            Self::Video => "视频",
            Self::Audio => "音频",
            Self::Image => "图片",
            Self::Archive => "压缩包",
            Self::Installer => "安装包",
            Self::Document => "文档",
            Self::Other => "其他",
        }
    }

    pub fn all() -> &'static [FileKind] {
        &[
            Self::All,
            Self::Video,
            Self::Audio,
            Self::Image,
            Self::Archive,
            Self::Installer,
            Self::Document,
            Self::Other,
        ]
    }
}

pub fn classify_path(path: &Path) -> FileKind {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "ts" | "rmvb" => {
            FileKind::Video
        }
        "mp3" | "flac" | "wav" | "aac" | "m4a" | "ogg" | "wma" | "ape" => FileKind::Audio,
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "heic" | "tif" | "tiff" | "raw"
        | "psd" => FileKind::Image,
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" | "img" | "cab" => {
            FileKind::Archive
        }
        "exe" | "msi" | "msix" | "appx" | "dmg" | "pkg" | "deb" | "rpm" => FileKind::Installer,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "csv" | "rtf"
        | "epub" => FileKind::Document,
        _ => FileKind::Other,
    }
}

pub fn filter_files<'a>(
    entries: impl Iterator<Item = &'a FsEntry>,
    kind: FileKind,
    min_size: u64,
    limit: usize,
) -> Vec<&'a FsEntry> {
    let mut v: Vec<&FsEntry> = entries
        .filter(|e| !e.is_dir)
        .filter(|e| e.size >= min_size)
        .filter(|e| kind == FileKind::All || classify_path(&e.path) == kind)
        .collect();
    v.sort_by(|a, b| b.size.cmp(&a.size));
    v.truncate(limit);
    v
}
