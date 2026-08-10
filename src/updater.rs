//! 远程更新：对齐 DeskReader / jiaoben app-update，本机自动下载热替换 + SHA256 校验

use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const APP_VERSION_NAME: &str = env!("CARGO_PKG_VERSION");
include!(concat!(env!("OUT_DIR"), "/version_code.rs"));
pub const APP_KEY: &str = "disk-janitor";
pub const DEFAULT_API_BASE: &str = "http://111.229.202.251:8687";

#[derive(Debug, Clone)]
pub struct RemoteManifest {
    pub version_code: u32,
    pub version_name: String,
    pub url: String,
    pub changelog: String,
    pub sha256: String,
}

impl RemoteManifest {
    pub fn label(&self) -> String {
        if self.version_name.is_empty() {
            format!("#{}", self.version_code)
        } else {
            format!("{} (#{})", self.version_name, self.version_code)
        }
    }
}

#[derive(Debug, Clone)]
pub enum UpdateCheck {
    UpToDate,
    Available(RemoteManifest),
    Disabled,
    Failed(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_api_base", alias = "update_url")]
    pub update_api_base: String,
    #[serde(default = "default_true")]
    pub check_on_start: bool,
    /// UI 缩放（1.0 / 1.15 / 1.3）
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    /// 上次扫描根路径（便于继续扫）
    #[serde(default)]
    pub last_scan_root: String,
    /// 上次扫描摘要一行
    #[serde(default)]
    pub last_scan_summary: String,
    #[serde(default)]
    pub dup_keep_strategy: crate::duplicates::KeepStrategy,
}

fn default_api_base() -> String {
    DEFAULT_API_BASE.to_string()
}

fn default_true() -> bool {
    true
}

fn default_ui_scale() -> f32 {
    1.0
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            update_api_base: DEFAULT_API_BASE.to_string(),
            check_on_start: true,
            ui_scale: 1.0,
            last_scan_root: String::new(),
            last_scan_summary: String::new(),
            dup_keep_strategy: crate::duplicates::KeepStrategy::PreferNotDownloads,
        }
    }
}

impl AppConfig {
    pub fn path() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("disk-janitor").join("config.json")
    }

    pub fn load() -> Self {
        let p = Self::path();
        let mut cfg = if let Ok(s) = fs::read_to_string(&p) {
            serde_json::from_str(&s).unwrap_or_default()
        } else {
            Self::default()
        };
        cfg.normalize();
        cfg
    }

    pub fn normalize(&mut self) {
        if !self.ui_scale.is_finite() {
            self.ui_scale = 1.0;
        } else {
            self.ui_scale = self.ui_scale.clamp(0.85, 2.0);
        }
        let mut base = self.update_api_base.trim().to_string();
        if base.is_empty() || base.contains("YOUR_SERVER") {
            self.update_api_base = DEFAULT_API_BASE.to_string();
            return;
        }
        while base.ends_with('/') {
            base.pop();
        }
        let suffixes = [
            "/latest.json",
            "/app-update.json",
            &format!("/api/{APP_KEY}/app-update"),
            &format!("/api/app-update/{APP_KEY}"),
            &format!("/{APP_KEY}/app-update.json"),
            &format!("/{APP_KEY}"),
        ];
        loop {
            let mut changed = false;
            for s in &suffixes {
                if let Some(stripped) = base.strip_suffix(*s) {
                    base = stripped.trim_end_matches('/').to_string();
                    changed = true;
                    break;
                }
            }
            if !changed {
                break;
            }
        }
        self.update_api_base = if base.is_empty() {
            DEFAULT_API_BASE.to_string()
        } else {
            base
        };
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::path();
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(p, s).map_err(|e| e.to_string())
    }
}

pub fn check_update(api_base: &str) -> UpdateCheck {
    let base = api_base.trim().trim_end_matches('/');
    if base.is_empty() {
        return UpdateCheck::Disabled;
    }
    match fetch_best_update(base) {
        Ok(None) => UpdateCheck::Failed("三源均未返回有效 app-update".into()),
        Ok(Some(m)) => {
            if need_update(&m) {
                UpdateCheck::Available(m)
            } else {
                UpdateCheck::UpToDate
            }
        }
        Err(e) => UpdateCheck::Failed(e),
    }
}

fn candidate_urls(base: &str) -> [String; 3] {
    [
        format!("{base}/{APP_KEY}/app-update.json"),
        format!("{base}/api/{APP_KEY}/app-update"),
        format!("{base}/api/app-update/{APP_KEY}"),
    ]
}

fn fetch_best_update(base: &str) -> Result<Option<RemoteManifest>, String> {
    let mut best: Option<(i64, RemoteManifest)> = None;
    let mut last_err = String::new();
    for url in candidate_urls(base) {
        match fetch_one(&url) {
            Ok(Some(m)) => {
                let sc = score_update(&m);
                if best.as_ref().map(|(s, _)| sc > *s).unwrap_or(true) {
                    best = Some((sc, m));
                }
            }
            Ok(None) => {}
            Err(e) => last_err = e,
        }
    }
    if best.is_none() && !last_err.is_empty() {
        return Err(last_err);
    }
    Ok(best.map(|(_, m)| m))
}

fn fetch_one(url: &str) -> Result<Option<RemoteManifest>, String> {
    let resp = ureq::get(url)
        .set(
            "User-Agent",
            concat!("disk-janitor/", env!("CARGO_PKG_VERSION")),
        )
        .set("Accept", "application/json")
        .call()
        .map_err(|e| format!("网络错误({url}): {e}"))?;
    if !(200..300).contains(&resp.status()) {
        return Ok(None);
    }
    let v: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("解析 JSON 失败({url}): {e}"))?;
    let payload = if v.get("data").is_some() {
        &v["data"]
    } else {
        &v
    };
    Ok(parse_update_payload(payload))
}

fn parse_update_payload(v: &serde_json::Value) -> Option<RemoteManifest> {
    if !v.is_object() {
        return None;
    }
    let version_code = v
        .get("versionCode")
        .or_else(|| v.get("androidVersionCode"))
        .and_then(|x| x.as_u64().or_else(|| x.as_str().and_then(|s| s.parse().ok())))
        .unwrap_or(0) as u32;
    let version_name = v
        .get("versionName")
        .or_else(|| v.get("webVersion"))
        .or_else(|| v.get("version"))
        .and_then(|x| match x {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .unwrap_or_default();
    let url = v
        .get("desktopUrl")
        .or_else(|| v.get("desktopSetupUrl"))
        .or_else(|| v.get("desktopPortableUrl"))
        .or_else(|| v.get("url"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let changelog = v
        .get("changelog")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let sha256 = v
        .get("sha256")
        .or_else(|| v.get("sha256sum"))
        .or_else(|| v.get("hash"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if version_code == 0 && version_name.is_empty() && url.is_empty() {
        return None;
    }
    Some(RemoteManifest {
        version_code,
        version_name,
        url,
        changelog,
        sha256,
    })
}

fn score_update(m: &RemoteManifest) -> i64 {
    let mut s = i64::from(m.version_code) * 1000;
    if !m.url.is_empty() {
        s += 30;
    }
    if !m.sha256.is_empty() {
        s += 20;
    }
    if !m.changelog.is_empty() && !m.changelog.contains("初始占位") {
        s += 5;
    }
    s
}

pub fn need_update(remote: &RemoteManifest) -> bool {
    if remote.version_code > APP_VERSION_CODE {
        return true;
    }
    if !remote.version_name.is_empty()
        && compare_version_name(&remote.version_name, APP_VERSION_NAME) > 0
    {
        return true;
    }
    false
}

pub fn compare_version_name(a: &str, b: &str) -> i32 {
    let parse = |s: &str| -> Vec<u64> {
        s.trim()
            .trim_start_matches('v')
            .split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let pa = parse(a);
    let pb = parse(b);
    let n = pa.len().max(pb.len());
    for i in 0..n {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x != y {
            return if x > y { 1 } else { -1 };
        }
    }
    0
}

#[cfg(test)]
pub fn version_newer(remote: &str, local: &str) -> bool {
    compare_version_name(remote, local) > 0
}

pub fn open_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("下载地址为空".into());
    }
    Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// 下载到 exe 同目录的 disk-janitor.new.exe，校验 SHA256（若清单提供），并启动替换脚本
pub fn download_and_apply(manifest: &RemoteManifest) -> Result<String, String> {
    if manifest.url.trim().is_empty() {
        return Err("远程未提供 desktopUrl".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().unwrap_or_else(|| Path::new("."));
    let new_path = dir.join("disk-janitor.new.exe");
    let script = dir.join("disk-janitor-apply-update.cmd");

    let resp = ureq::get(manifest.url.trim())
        .set(
            "User-Agent",
            concat!("disk-janitor/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|e| format!("下载失败: {e}"))?;
    let mut reader = resp.into_reader();
    let mut file = fs::File::create(&new_path).map_err(|e| e.to_string())?;
    std::io::copy(&mut reader, &mut file).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    if !manifest.sha256.is_empty() {
        let got = sha256_file(&new_path)?;
        let expect = manifest.sha256.trim().to_ascii_lowercase();
        if got != expect {
            let _ = fs::remove_file(&new_path);
            return Err(format!(
                "SHA256 校验失败：期望 {expect}，实际 {got}。已删除损坏文件。"
            ));
        }
    }

    let exe_name = exe
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("disk-janitor.exe");
    let body = format!(
        "@echo off\r\n\
         chcp 65001 >nul\r\n\
         echo 正在应用更新…\r\n\
         timeout /t 2 /nobreak >nul\r\n\
         move /y \"%~dp0disk-janitor.new.exe\" \"%~dp0{exe_name}\"\r\n\
         if errorlevel 1 (\r\n\
           echo 替换失败，请手动把 disk-janitor.new.exe 改名为 {exe_name}\r\n\
           pause\r\n\
           exit /b 1\r\n\
         )\r\n\
         start \"\" \"%~dp0{exe_name}\"\r\n\
         del \"%~f0\"\r\n"
    );
    fs::write(&script, body).map_err(|e| e.to_string())?;

    Command::new("cmd")
        .args(["/C", "start", "", &script.to_string_lossy()])
        .spawn()
        .map_err(|e| e.to_string())?;

    let verify = if manifest.sha256.is_empty() {
        "（清单未提供 sha256，已跳过校验）"
    } else {
        "（SHA256 已校验）"
    };
    Ok(format!(
        "已下载 {} {}，即将重启替换。请保存工作后关闭本窗口。",
        manifest.label(),
        verify
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(version_newer("0.2.0", "0.1.0"));
        assert!(version_newer("1.0.0", "0.9.9"));
        assert!(!version_newer("0.1.0", "0.1.0"));
        assert!(!version_newer("0.1.0", "0.2.0"));
        assert_eq!(compare_version_name("0.2.0", "0.2.0"), 0);
    }

    #[test]
    fn need_update_by_code_or_name() {
        let by_code = RemoteManifest {
            version_code: APP_VERSION_CODE + 1,
            version_name: APP_VERSION_NAME.to_string(),
            url: "http://x/a.exe".into(),
            changelog: String::new(),
            sha256: String::new(),
        };
        assert!(need_update(&by_code));

        let same = RemoteManifest {
            version_code: APP_VERSION_CODE,
            version_name: APP_VERSION_NAME.to_string(),
            url: "http://x/a.exe".into(),
            changelog: String::new(),
            sha256: String::new(),
        };
        assert!(!need_update(&same));
    }

    #[test]
    fn parses_jiaoben_style_manifest() {
        let j = r#"{"versionCode":5,"versionName":"0.3.0","desktopUrl":"http://x/a.exe","changelog":"fix","sha256":"abc"}"#;
        let v: serde_json::Value = serde_json::from_str(j).unwrap();
        let m = parse_update_payload(&v).unwrap();
        assert_eq!(m.version_code, 5);
        assert_eq!(m.version_name, "0.3.0");
        assert_eq!(m.url, "http://x/a.exe");
        assert_eq!(m.sha256, "abc");
    }

    #[test]
    fn parses_wrapped_data() {
        let j = r#"{"code":200,"data":{"versionCode":2,"versionName":"0.2.0","desktopUrl":"http://y/b.exe"}}"#;
        let v: serde_json::Value = serde_json::from_str(j).unwrap();
        let payload = &v["data"];
        let m = parse_update_payload(payload).unwrap();
        assert_eq!(m.version_code, 2);
        assert_eq!(m.url, "http://y/b.exe");
    }

    #[test]
    fn candidate_urls_match_reader_shape() {
        let u = candidate_urls("http://host:8687");
        assert_eq!(u[0], "http://host:8687/disk-janitor/app-update.json");
        assert_eq!(u[1], "http://host:8687/api/disk-janitor/app-update");
        assert_eq!(u[2], "http://host:8687/api/app-update/disk-janitor");
    }

    #[test]
    fn normalize_strips_json_path() {
        let mut c = AppConfig {
            update_api_base: "http://h:8687/disk-janitor/latest.json".into(),
            check_on_start: true,
            ..Default::default()
        };
        c.normalize();
        assert_eq!(c.update_api_base, "http://h:8687");
    }
}
