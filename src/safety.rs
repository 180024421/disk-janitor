//! 破坏性文件操作的统一安全边界。

use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetState {
    Present,
    Missing,
}

/// 删除前校验目标。缺失目标按幂等成功处理；关键根目录和重解析点硬阻止。
pub fn validate_delete_target(path: &Path) -> Result<TargetState, String> {
    if path.as_os_str().is_empty() {
        return Err("目标路径为空".into());
    }
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let normalized = normalize(&absolute);
    if is_drive_root(&absolute)
        || protected_roots()
            .iter()
            .any(|root| normalize(root) == normalized)
    {
        return Err(format!(
            "安全策略拒绝删除关键根目录：{}",
            absolute.display()
        ));
    }
    if !absolute.exists() {
        return Ok(TargetState::Missing);
    }
    reject_reparse_path(&absolute)?;
    let final_path = absolute
        .canonicalize()
        .map_err(|e| format!("无法解析最终路径 {}：{e}", absolute.display()))?;
    let final_norm = normalize(&final_path);
    if is_drive_root(&final_path)
        || protected_roots()
            .iter()
            .any(|root| normalize(root) == final_norm)
    {
        return Err(format!(
            "目标最终指向关键根目录，已阻止：{}",
            final_path.display()
        ));
    }
    Ok(TargetState::Present)
}

fn reject_reparse_path(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

    for ancestor in path.ancestors() {
        if is_drive_root(ancestor) {
            break;
        }
        let Ok(meta) = std::fs::symlink_metadata(ancestor) else {
            continue;
        };
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(format!(
                "安全策略拒绝删除经过重解析点的路径：{}",
                ancestor.display()
            ));
        }
    }
    Ok(())
}

fn protected_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from(r"C:\Windows"),
        PathBuf::from(r"C:\Users"),
        PathBuf::from(r"C:\Program Files"),
        PathBuf::from(r"C:\Program Files (x86)"),
        PathBuf::from(r"C:\ProgramData"),
        PathBuf::from(r"C:\Recovery"),
        PathBuf::from(r"C:\Boot"),
        PathBuf::from(r"C:\EFI"),
        PathBuf::from(r"C:\System Volume Information"),
        PathBuf::from(r"C:\$Recycle.Bin"),
    ];
    for key in [
        "SystemRoot",
        "USERPROFILE",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
    ] {
        if let Some(value) = std::env::var_os(key) {
            roots.push(PathBuf::from(value));
        }
    }
    roots
}

fn normalize(path: &Path) -> String {
    let mut value = path
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    if value.len() == 2 && value.as_bytes()[1] == b':' {
        value.push('\\');
    }
    value
}

fn is_drive_root(path: &Path) -> bool {
    let mut components = path.components();
    matches!(
        (components.next(), components.next(), components.next()),
        (Some(Component::Prefix(_)), Some(Component::RootDir), None)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_drive_and_system_roots() {
        assert!(validate_delete_target(Path::new(r"C:\")).is_err());
        assert!(validate_delete_target(Path::new(r"C:\Windows")).is_err());
        assert!(validate_delete_target(Path::new(r"C:\Users")).is_err());
    }

    #[test]
    fn missing_non_root_is_idempotent() {
        let path = std::env::temp_dir()
            .join("disk-janitor-definitely-missing")
            .join("file");
        assert_eq!(validate_delete_target(&path).unwrap(), TargetState::Missing);
    }

    #[test]
    fn allows_regular_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("safe.txt");
        std::fs::write(&file, b"safe").unwrap();
        assert_eq!(validate_delete_target(&file).unwrap(), TargetState::Present);
    }
}
