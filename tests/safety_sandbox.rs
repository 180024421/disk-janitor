#![cfg(windows)]

#[path = "../src/safety.rs"]
mod safety;

use safety::{validate_delete_target, TargetState};
use std::path::Path;

#[test]
fn accepts_chinese_path_inside_temp_sandbox() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let dir = sandbox.path().join("中文目录").join("清理候选");
    std::fs::create_dir_all(&dir).expect("create Chinese path");
    let file = dir.join("缓存文件.txt");
    std::fs::write(&file, b"sandbox-only").expect("write sandbox fixture");

    assert_eq!(
        validate_delete_target(&file).expect("sandbox file should be accepted"),
        TargetState::Present
    );
    assert!(file.exists(), "validation must not modify its target");
}

#[test]
fn blocks_critical_paths_without_accessing_them() {
    for path in [
        r"C:\",
        r"C:\Windows",
        r"c:/windows/",
        r"C:\Users",
        r"C:\Program Files",
        r"C:\ProgramData",
        r"C:\$Recycle.Bin",
    ] {
        assert!(
            validate_delete_target(Path::new(path)).is_err(),
            "{path} must be rejected"
        );
    }
}

#[test]
fn rejects_reparse_ancestor_and_preserves_target() {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let target = sandbox.path().join("真实目录");
    std::fs::create_dir(&target).expect("create target");
    let marker = target.join("必须保留.txt");
    std::fs::write(&marker, b"keep").expect("write marker");

    let link = sandbox.path().join("重解析入口");
    if let Err(error) = std::os::windows::fs::symlink_dir(&target, &link) {
        // Some self-hosted runners do not grant CreateSymbolicLink privilege.
        eprintln!("skipping reparse assertion: {error}");
        return;
    }

    let through_link = link.join("必须保留.txt");
    assert!(
        validate_delete_target(&through_link).is_err(),
        "a path through a reparse point must be rejected"
    );
    assert_eq!(
        std::fs::read(&marker).expect("target must remain readable"),
        b"keep"
    );
}
