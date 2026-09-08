//! 项目自有 JSON 数据的版本包装与原子持久化。

use serde::de::DeserializeOwned;
use serde::Serialize;
#[cfg(unix)]
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const SCHEMA_VERSION: u32 = 1;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize)]
struct VersionedRef<'a, T: ?Sized> {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    data: &'a T,
}

pub fn save_json<T: Serialize + ?Sized>(
    path: &Path,
    value: &T,
    pretty: bool,
) -> Result<(), String> {
    let wrapped = VersionedRef {
        schema_version: SCHEMA_VERSION,
        data: value,
    };
    let bytes = if pretty {
        serde_json::to_vec_pretty(&wrapped)
    } else {
        serde_json::to_vec(&wrapped)
    }
    .map_err(|e| e.to_string())?;
    atomic_write(path, &bytes)
}

pub fn load_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
    decode_json(&bytes)
}

pub fn decode_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let Some(object) = value.as_object() else {
        return serde_json::from_value(value).map_err(|e| e.to_string());
    };
    let version = object
        .get("schemaVersion")
        .or_else(|| object.get("schema_version"));
    if let Some(version) = version {
        let version = version
            .as_u64()
            .ok_or_else(|| "schemaVersion/schema_version 无效".to_string())?;
        if version > SCHEMA_VERSION as u64 {
            return Err(format!("不支持的 schemaVersion: {version}"));
        }
        let data = object
            .get("data")
            .cloned()
            .ok_or_else(|| "版本化 JSON 缺少 data".to_string())?;
        serde_json::from_value(data).map_err(|e| e.to_string())
    } else {
        // v0 数据没有包装层，保持直接反序列化兼容。
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("无效持久化路径: {}", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("创建目录 {} 失败: {e}", parent.display()))?;

    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("data.json");
    let temp = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), sequence));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| format!("创建临时文件 {} 失败: {e}", temp.display()))?;
        file.write_all(bytes)
            .map_err(|e| format!("写入临时文件 {} 失败: {e}", temp.display()))?;
        file.flush()
            .map_err(|e| format!("flush 临时文件 {} 失败: {e}", temp.display()))?;
        file.sync_all()
            .map_err(|e| format!("sync 临时文件 {} 失败: {e}", temp.display()))?;
        drop(file);
        replace_file(&temp, path)?;
        sync_parent(parent);
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::rename(from, to).map_err(|e| format!("原子替换 {} 失败: {e}", to.display()))
}

#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    if !to.exists() {
        return std::fs::rename(from, to)
            .map_err(|e| format!("原子替换 {} 失败: {e}", to.display()));
    }

    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    #[link(name = "kernel32")]
    extern "system" {
        fn ReplaceFileW(
            replaced_file_name: *const u16,
            replacement_file_name: *const u16,
            backup_file_name: *const u16,
            replace_flags: u32,
            exclude: *mut std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
        ) -> i32;
    }

    let target: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    let replacement: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let replaced = unsafe {
        ReplaceFileW(
            target.as_ptr(),
            replacement.as_ptr(),
            ptr::null(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    if replaced == 0 {
        Err(format!(
            "原子替换 {} 失败: {}",
            to.display(),
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_parent(parent: &Path) {
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) {}

pub fn preserve_corrupt(path: &Path) {
    let base = PathBuf::from(format!("{}.corrupt", path.display()));
    let backup = if !base.exists() {
        base
    } else {
        (1u32..)
            .map(|n| PathBuf::from(format!("{}.corrupt.{n}", path.display())))
            .find(|candidate| !candidate.exists())
            .unwrap_or_else(|| PathBuf::from(format!("{}.corrupt.last", path.display())))
    };
    if std::fs::rename(path, &backup).is_err() && std::fs::copy(path, &backup).is_ok() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Sample {
        value: u32,
    }

    #[test]
    fn reads_camel_snake_and_legacy_schemas() {
        let expected = Sample { value: 7 };
        for json in [
            r#"{"schemaVersion":1,"data":{"value":7}}"#,
            r#"{"schema_version":1,"data":{"value":7}}"#,
            r#"{"value":7}"#,
        ] {
            assert_eq!(decode_json::<Sample>(json.as_bytes()).unwrap(), expected);
        }
    }

    #[test]
    fn atomic_versioned_round_trip_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.json");
        save_json(&path, &Sample { value: 1 }, true).unwrap();
        save_json(&path, &Sample { value: 2 }, true).unwrap();
        assert_eq!(load_json::<Sample>(&path).unwrap().value, 2);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
