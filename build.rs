// 从 Cargo.toml 生成唯一版本码，避免与 version 手工不同步
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let toml = fs::read_to_string(format!("{manifest}/Cargo.toml")).expect("Cargo.toml");
    let mut version = "0.0.0".to_string();
    for line in toml.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("version") {
            if let Some(v) = rest.split('=').nth(1) {
                version = v.trim().trim_matches('"').to_string();
                break;
            }
        }
    }
    let code = version_code_from_name(&version);
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("version_code.rs");
    fs::write(
        &out,
        format!("pub const APP_VERSION_CODE: u32 = {code};\n"),
    )
    .expect("write version_code.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
}

fn version_code_from_name(v: &str) -> u32 {
    let parts: Vec<u32> = v
        .trim()
        .trim_start_matches('v')
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect();
    let major = parts.first().copied().unwrap_or(0);
    let minor = parts.get(1).copied().unwrap_or(0);
    let patch = parts.get(2).copied().unwrap_or(0);
    major * 10000 + minor * 100 + patch
}
