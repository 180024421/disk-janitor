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
    if let Some(owner) = protected_subtree_owner(&normalized) {
        return Err(format!(
            "安全策略拒绝删除关键系统目录「{owner}」之下的路径：{}",
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
        || protected_subtree_owner(&final_norm).is_some()
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
        let meta = match std::fs::symlink_metadata(ancestor) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(format!(
                    "无法确认路径祖先是否为重解析点，已按不安全处理：{}（{e}）",
                    ancestor.display()
                ))
            }
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

/// 命中关键系统子树（含其子孙）时返回所属子树名。
/// 注意：`C:\Users`、`Program Files` 等只拦根本身，其下的 Temp/缓存/卸载目录是正常清理对象。
fn protected_subtree_owner(path_norm: &str) -> Option<String> {
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let sys = normalize(&system_root);
    for child in ["system32", "syswow64", "winsxs", "servicing", "boot", "installer"] {
        let sub = format!(r"{sys}\{child}");
        if path_norm == sub || path_norm.starts_with(&format!(r"{sub}\")) {
            return Some(sub);
        }
    }
    // 任意盘符下的这些一级目录整体受保护（回收站、卷影信息、恢复分区目录等）。
    let mut segs = path_norm.split('\\');
    let drive = segs.next()?;
    if drive.len() != 2 || !drive.ends_with(':') {
        return None;
    }
    let first = segs.next()?;
    if [
        "$recycle.bin",
        "system volume information",
        "recovery",
        "efi",
        "boot",
    ]
    .iter()
    .any(|name| *name == first)
    {
        return Some(format!(r"{drive}\{first}"));
    }
    None
}

fn normalize(path: &Path) -> String {
    let mut value = path.to_string_lossy().replace('/', "\\");
    // canonicalize() 返回 \\?\ verbatim 路径，比对前必须剥离，否则防护永不命中。
    if let Some(rest) = value.strip_prefix(r"\\?\") {
        value = if rest.len() >= 4 && rest[..4].eq_ignore_ascii_case("unc\\") {
            format!(r"\\{}", &rest[4..])
        } else {
            rest.to_string()
        };
    }
    let mut value = value
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
    fn blocks_descendants_of_critical_system_subtrees() {
        for path in [
            r"C:\Windows\System32\drivers\etc\hosts",
            r"C:/WINDOWS/system32/config/SAM",
            r"C:\Windows\WinSxS\amd64foo",
            r"C:\$Recycle.Bin\S-1-5-21-1\$Iabc.txt",
            r"D:\$Recycle.Bin\S-1-5-21-1",
            r"E:\System Volume Information\stuff",
            r"C:\Recovery\WindowsRE",
        ] {
            assert!(
                validate_delete_target(Path::new(path)).is_err(),
                "{path} must be rejected as protected-subtree descendant"
            );
        }
    }

    #[test]
    fn still_allows_junk_areas_under_broad_roots() {
        // 这些是清理主业务，不能被前缀拦截误杀（仅存在性检查，不触碰内容）。
        for path in [
            r"C:\Users\someuser\AppData\Local\Temp\cache.bin",
            r"C:\Windows\Temp\setup.log",
            r"C:\Program Files\Vendor\App\unins000.exe",
        ] {
            let result = validate_delete_target(Path::new(path));
            assert!(
                !matches!(&result, Err(msg) if msg.contains("关键")),
                "{path} wrongly blocked: {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn normalize_strips_verbatim_prefix() {
        assert_eq!(normalize(Path::new(r"\\?\C:\Windows")), "c:\\windows");
        assert_eq!(
            normalize(Path::new(r"\\?\C:\Windows\System32")),
            "c:\\windows\\system32"
        );
        assert_eq!(
            normalize(Path::new(r"\\?\UNC\server\share")),
            r"\\server\share"
        );
        assert_eq!(normalize(Path::new("c:/users/")), "c:\\users");
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
