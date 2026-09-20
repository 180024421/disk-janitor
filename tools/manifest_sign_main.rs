//! 发布清单签名工具：为 app-update.json 生成/校验 Ed25519 签名。
//!
//! 私钥绝不入库。打包机本地保存（如 %LOCALAPPDATA%\disk-janitor-release\manifest-sign.key，
//! 文件内容为一行 64 位 hex 种子），也可用环境变量 DJ_MANIFEST_SIGN_KEY 传入。
//!
//! 用法：
//!   dj-manifest-sign gen                          生成新密钥对（轮换时用）
//!   dj-manifest-sign sign <file.json> [--key <f>] 就地写入 sig 字段
//!   dj-manifest-sign verify <file.json>           用内置公钥校验

use disk_janitor::updater::{
    parse_update_payload, sign_manifest_with, verify_manifest_signature, MANIFEST_PUBLIC_KEY_HEX,
};
use std::process::ExitCode;

fn print_usage() {
    eprintln!("用法:\n  dj-manifest-sign gen\n  dj-manifest-sign sign <file.json> [--key <keyfile>]\n  dj-manifest-sign verify <file.json>");
}

fn load_key(key_arg: Option<&String>) -> Result<String, String> {
    let path = match key_arg {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            if let Ok(v) = std::env::var("DJ_MANIFEST_SIGN_KEY") {
                return Ok(v.trim().to_string());
            }
            let base = std::env::var_os("LOCALAPPDATA")
                .ok_or_else(|| "LOCALAPPDATA 不可用，请用 --key 指定私钥文件".to_string())?;
            std::path::PathBuf::from(base)
                .join("disk-janitor-release")
                .join("manifest-sign.key")
        }
    };
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("无法读取私钥文件 {}：{e}", path.display()))?;
    content
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .ok_or_else(|| "私钥文件为空".to_string())
}

/// 从 JSON 中定位清单实体（兼容 {code,data:{...}} 包装）。
fn payload_slot(value: &mut serde_json::Value) -> &mut serde_json::Value {
    if value.get("data").map(|d| d.is_object()).unwrap_or(false) {
        &mut value["data"]
    } else {
        value
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("gen") => {
            use ed25519_dalek::SigningKey;
            use rand_core::{OsRng, RngCore};
            let mut seed = [0u8; 32];
            OsRng.fill_bytes(&mut seed);
            let key = SigningKey::from_bytes(&seed);
            println!(
                "public(hex): {}",
                hex::encode(key.verifying_key().to_bytes())
            );
            println!("private(hex): {}", hex::encode(key.to_bytes()));
            println!("请把 public 更新到 updater.rs::MANIFEST_PUBLIC_KEY_HEX，私钥仅保存在打包机。");
            Ok(())
        }
        Some("sign") => {
            let file = args.get(1).ok_or_else(|| "缺少 <file.json> 参数".to_string())?;
            let key_arg = args
                .iter()
                .position(|a| a == "--key")
                .and_then(|i| args.get(i + 1));
            let key_hex = load_key(key_arg)?;
            let raw = std::fs::read_to_string(file).map_err(|e| format!("读取失败：{e}"))?;
            let mut json: serde_json::Value =
                serde_json::from_str(&raw).map_err(|e| format!("JSON 解析失败：{e}"))?;
            let payload = payload_slot(&mut json).clone();
            let mut manifest =
                parse_update_payload(&payload).ok_or("文件中没有可解析的清单字段")?;
            sign_manifest_with(&mut manifest, &key_hex)?;
            payload_slot(&mut json)["sig"] = serde_json::Value::String(manifest.sig.clone());
            std::fs::write(file, serde_json::to_string_pretty(&json).unwrap())
                .map_err(|e| format!("写回失败：{e}"))?;
            println!("已签名 sig={}…", &manifest.sig[..16.min(manifest.sig.len())]);
            Ok(())
        }
        Some("verify") => {
            let file = args.get(1).ok_or_else(|| "缺少 <file.json> 参数".to_string())?;
            let raw = std::fs::read_to_string(file).map_err(|e| format!("读取失败：{e}"))?;
            let json: serde_json::Value =
                serde_json::from_str(&raw).map_err(|e| format!("JSON 解析失败：{e}"))?;
            let payload = payload_slot(&mut json.clone()).clone();
            let manifest = parse_update_payload(&payload).ok_or("文件中没有可解析的清单字段")?;
            verify_manifest_signature(&manifest, MANIFEST_PUBLIC_KEY_HEX)?;
            println!("签名有效：{} ({})", manifest.version_name, manifest.version_code);
            Ok(())
        }
        _ => {
            print_usage();
            Err("无子命令".to_string())
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("错误：{e}");
            ExitCode::FAILURE
        }
    }
}
