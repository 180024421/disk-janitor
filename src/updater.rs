//! 远程更新：对齐 DeskReader / jiaoben app-update，本机自动下载热替换 + SHA256 校验

use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const APP_VERSION_NAME: &str = env!("CARGO_PKG_VERSION");
include!(concat!(env!("OUT_DIR"), "/version_code.rs"));
pub const APP_KEY: &str = "disk-janitor";
pub const MAX_UPDATE_BYTES: u64 = 300 * 1024 * 1024;
/// 与 DeskReader 一致：花生壳 HTTPS → Nginx → jiaoben（勿带 :8687）
pub const DEFAULT_API_BASE: &str = "https://1ph1hf8043323.vicp.fun";

/// 历史占位 / 直连 IP:8687 → 统一到花生壳域名
pub fn rewrite_public_host(url: &str) -> String {
    let mut u = url.trim().to_string();
    if u.is_empty() {
        return u;
    }
    u = u.replace("YOUR_SERVER_IP", "1ph1hf8043323.vicp.fun");
    for old in [
        "http://111.229.202.251:8687",
        "https://111.229.202.251:8687",
        "http://111.229.202.251",
        "https://111.229.202.251",
        "http://1ph1hf8043323.vicp.fun:8687",
        "https://1ph1hf8043323.vicp.fun:8687",
    ] {
        u = u.replace(old, DEFAULT_API_BASE);
    }
    if u.starts_with("http://1ph1hf8043323.vicp.fun") {
        u = u.replacen("http://", "https://", 1);
    }
    u
}

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

/// 清单签名验证扩展点。staged 版本传入 `None`，后续可注入强制签名验证实现。
pub trait ManifestSignatureVerifier {
    fn verify(&self, manifest: &RemoteManifest) -> Result<(), String>;
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
    /// "system" | "dark" | "light"
    #[serde(default = "default_theme_mode")]
    pub theme_mode: String,
    /// "comfortable" | "compact"
    #[serde(default = "default_ui_density")]
    pub ui_density: String,
    /// 上次扫描根路径（便于继续扫）
    #[serde(default)]
    pub last_scan_root: String,
    /// 上次扫描摘要一行
    #[serde(default)]
    pub last_scan_summary: String,
    #[serde(default)]
    pub dup_keep_strategy: crate::duplicates::KeepStrategy,
    /// 查重最小文件体积（MB）
    #[serde(default = "default_dup_min_mb")]
    pub dup_min_mb: u64,
    /// 查重最多返回组数
    #[serde(default = "default_dup_max_groups")]
    pub dup_max_groups: usize,
    /// 启动后安静清理安全垃圾（仅一次）
    #[serde(default)]
    pub quiet_clean_on_start: bool,
    /// 浏览页显示 Treemap
    #[serde(default = "default_true")]
    pub show_treemap: bool,
    /// 扫描排除路径（前缀匹配）
    #[serde(default)]
    pub exclude_paths: Vec<String>,
    /// "normal" | "turbo"
    #[serde(default = "default_scan_mode")]
    pub scan_mode: String,
    /// 是否启用每日安静清理计划任务
    #[serde(default)]
    pub schedule_quiet_clean: bool,
    /// 计划任务时间 HH:MM
    #[serde(default = "default_schedule_time")]
    pub schedule_time: String,
    /// 删除确认默认勾选安全粉碎
    #[serde(default)]
    pub shred_default: bool,
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

fn default_theme_mode() -> String {
    "system".into()
}

fn default_ui_density() -> String {
    "comfortable".into()
}

fn default_dup_min_mb() -> u64 {
    1
}

fn default_dup_max_groups() -> usize {
    80
}

fn default_scan_mode() -> String {
    "normal".into()
}

fn default_schedule_time() -> String {
    "03:00".into()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            update_api_base: DEFAULT_API_BASE.to_string(),
            check_on_start: true,
            ui_scale: 1.0,
            theme_mode: default_theme_mode(),
            ui_density: default_ui_density(),
            last_scan_root: String::new(),
            last_scan_summary: String::new(),
            dup_keep_strategy: crate::duplicates::KeepStrategy::PreferNotDownloads,
            dup_min_mb: 1,
            dup_max_groups: 80,
            quiet_clean_on_start: false,
            show_treemap: true,
            exclude_paths: Vec::new(),
            scan_mode: "normal".into(),
            schedule_quiet_clean: false,
            schedule_time: "03:00".into(),
            shred_default: false,
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
        let mut cfg = crate::persistence::load_json(&p).unwrap_or_default();
        cfg.normalize();
        cfg
    }

    pub fn normalize(&mut self) {
        if !self.ui_scale.is_finite() {
            self.ui_scale = 1.0;
        } else {
            self.ui_scale = self.ui_scale.clamp(0.85, 2.0);
        }
        self.theme_mode = match self.theme_mode.trim().to_ascii_lowercase().as_str() {
            "dark" => "dark".into(),
            "light" => "light".into(),
            _ => "system".into(),
        };
        self.ui_density = if self.ui_density.eq_ignore_ascii_case("compact") {
            "compact".into()
        } else {
            "comfortable".into()
        };
        if self.dup_min_mb == 0 {
            self.dup_min_mb = 1;
        }
        if self.dup_max_groups == 0 {
            self.dup_max_groups = 80;
        }
        let mode = self.scan_mode.trim().to_ascii_lowercase();
        self.scan_mode = if mode == "turbo" {
            "turbo".into()
        } else {
            "normal".into()
        };
        if self.schedule_time.trim().is_empty() {
            self.schedule_time = "03:00".into();
        }
        let mut base = rewrite_public_host(&self.update_api_base);
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
        crate::persistence::save_json(&Self::path(), self, true)
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
    // jiaoben 公开 API 优先（与 DeskReader 一致）；静态 json / 旧路径仅作兜底
    [
        format!("{base}/api/app-update/{APP_KEY}"),
        format!("{base}/{APP_KEY}/app-update.json"),
        format!("{base}/api/{APP_KEY}/app-update"),
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
    let resp = match ureq::get(url)
        .set(
            "User-Agent",
            concat!("disk-janitor/", env!("CARGO_PKG_VERSION")),
        )
        .set("Accept", "application/json")
        .call()
    {
        Ok(r) if (200..300).contains(&r.status()) => r,
        Ok(_) | Err(ureq::Error::Status(404, _)) | Err(ureq::Error::Status(403, _)) => {
            return Ok(None);
        }
        Err(e) => return Err(format!("网络错误({url}): {e}")),
    };
    let v: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let payload = if v.get("data").is_some() {
        &v["data"]
    } else {
        &v
    };
    let manifest = match parse_update_payload(payload) {
        Some(manifest) => manifest,
        None => return Ok(None),
    };
    validate_remote_manifest(&manifest, None)?;
    Ok(Some(manifest))
}

fn parse_update_payload(v: &serde_json::Value) -> Option<RemoteManifest> {
    if !v.is_object() {
        return None;
    }
    let version_code = v
        .get("versionCode")
        .or_else(|| v.get("androidVersionCode"))
        .and_then(|x| {
            x.as_u64()
                .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
        })
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
    let url = rewrite_public_host(
        v.get("desktopUrl")
            .or_else(|| v.get("desktopSetupUrl"))
            .or_else(|| v.get("desktopPortableUrl"))
            .or_else(|| v.get("url"))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim(),
    );
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
    validate_https_url(url)?;
    Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn validate_https_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    if !url
        .get(.."https://".len())
        .map(|prefix| prefix.eq_ignore_ascii_case("https://"))
        .unwrap_or(false)
        || url.len() <= "https://".len()
    {
        return Err("更新下载地址必须是 HTTPS URL".into());
    }
    let authority = url[8..].split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains(char::is_whitespace) {
        return Err("更新下载地址不是合法的 HTTPS URL".into());
    }
    Ok(())
}

pub fn validate_sha256(value: &str) -> Result<String, String> {
    let hash = value.trim();
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("远程更新必须提供合法的 64 位十六进制 SHA256".into());
    }
    Ok(hash.to_ascii_lowercase())
}

pub fn validate_remote_manifest(
    manifest: &RemoteManifest,
    signature_verifier: Option<&dyn ManifestSignatureVerifier>,
) -> Result<(), String> {
    validate_https_url(&manifest.url)?;
    validate_sha256(&manifest.sha256)?;
    if let Some(verifier) = signature_verifier {
        verifier.verify(manifest)?;
    }
    Ok(())
}

fn is_allowed_binary_content_type(value: Option<&str>) -> bool {
    let media_type = value
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    matches!(
        media_type.as_str(),
        "application/octet-stream"
            | "binary/octet-stream"
            | "application/x-msdownload"
            | "application/x-msdos-program"
            | "application/vnd.microsoft.portable-executable"
            | "application/x-executable"
    )
}

fn checked_download_size(current: u64, chunk: usize) -> Result<u64, String> {
    let next = current
        .checked_add(chunk as u64)
        .ok_or_else(|| "更新文件大小溢出".to_string())?;
    if next > MAX_UPDATE_BYTES {
        return Err(format!(
            "更新文件超过最大限制 {} MB",
            MAX_UPDATE_BYTES / 1024 / 1024
        ));
    }
    Ok(next)
}

fn validate_content_length(length: u64) -> Result<(), String> {
    if length > MAX_UPDATE_BYTES {
        return Err(format!(
            "更新文件超过最大限制 {} MB",
            MAX_UPDATE_BYTES / 1024 / 1024
        ));
    }
    Ok(())
}

fn write_limited<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> Result<u64, String> {
    let mut total = 0u64;
    let mut buf = [0u8; 256 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total = checked_download_size(total, n)?;
        writer.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    Ok(total)
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

/// 下载到 exe 同目录临时文件，校验 SHA256，并启动带备份、失败回滚的替换脚本。
pub fn download_and_apply(manifest: &RemoteManifest) -> Result<String, String> {
    validate_remote_manifest(manifest, None)?;
    let expected_sha256 = validate_sha256(&manifest.sha256)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().unwrap_or_else(|| Path::new("."));
    let nonce = format!("{}-{}", std::process::id(), manifest.version_code);
    let new_name = format!("disk-janitor.{nonce}.new.exe");
    let new_path = dir.join(&new_name);
    let script = dir.join("disk-janitor-apply-update.cmd");
    let restore_script = dir.join("disk-janitor-restore-previous.cmd");

    let resp = ureq::get(manifest.url.trim())
        .set(
            "User-Agent",
            concat!("disk-janitor/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|e| format!("下载失败: {e}"))?;
    validate_https_url(resp.get_url())?;
    if !is_allowed_binary_content_type(resp.header("Content-Type")) {
        return Err(format!(
            "更新响应 Content-Type 不受支持: {}",
            resp.header("Content-Type").unwrap_or("<missing>")
        ));
    }
    if let Some(length) = resp
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok())
    {
        validate_content_length(length)?;
    }
    let mut reader = resp.into_reader();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&new_path)
        .map_err(|e| format!("无法在程序目录创建更新临时文件: {e}"))?;
    if let Err(e) = write_limited(&mut reader, &mut file) {
        drop(file);
        let _ = fs::remove_file(&new_path);
        return Err(e);
    }
    file.flush().map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);

    let got = sha256_file(&new_path)?;
    if got != expected_sha256 {
        let _ = fs::remove_file(&new_path);
        return Err(format!(
            "SHA256 校验失败：期望 {expected_sha256}，实际 {got}。已删除损坏文件。"
        ));
    }

    let exe_name = exe
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("disk-janitor.exe");
    let backup_name = format!("{exe_name}.previous");
    let restore_body = format!(
        "@echo off\r\n\
         chcp 65001 >nul\r\n\
         if not exist \"%~dp0{backup_name}\" (\r\n\
           echo 未找到旧版本备份：%~dp0{backup_name}\r\n\
           pause\r\n\
           exit /b 1\r\n\
         )\r\n\
         taskkill /f /im \"{exe_name}\" >nul 2>&1\r\n\
         timeout /t 1 /nobreak >nul\r\n\
         if exist \"%~dp0{exe_name}.failed\" del /f /q \"%~dp0{exe_name}.failed\"\r\n\
         if exist \"%~dp0{exe_name}\" move /y \"%~dp0{exe_name}\" \"%~dp0{exe_name}.failed\" >nul\r\n\
         move /y \"%~dp0{backup_name}\" \"%~dp0{exe_name}\" >nul\r\n\
         if errorlevel 1 (\r\n\
           echo 恢复失败，备份仍位于：%~dp0{backup_name}\r\n\
           pause\r\n\
           exit /b 2\r\n\
         )\r\n\
         start \"\" \"%~dp0{exe_name}\"\r\n\
         echo 已恢复旧版本。\r\n"
    );
    fs::write(&restore_script, restore_body).map_err(|e| e.to_string())?;
    let body = format!(
        "@echo off\r\n\
         chcp 65001 >nul\r\n\
         echo 正在应用更新…\r\n\
         timeout /t 2 /nobreak >nul\r\n\
         if exist \"%~dp0{backup_name}\" del /f /q \"%~dp0{backup_name}\"\r\n\
         move /y \"%~dp0{exe_name}\" \"%~dp0{backup_name}\" >nul\r\n\
         if errorlevel 1 goto rollback_failed\r\n\
         move /y \"%~dp0{new_name}\" \"%~dp0{exe_name}\" >nul\r\n\
         if errorlevel 1 goto rollback\r\n\
         start \"\" \"%~dp0{exe_name}\"\r\n\
         del \"%~f0\"\r\n\
         exit /b 0\r\n\
         :rollback\r\n\
         move /y \"%~dp0{backup_name}\" \"%~dp0{exe_name}\" >nul\r\n\
         if errorlevel 1 goto rollback_failed\r\n\
         echo 更新失败，已恢复旧版本。\r\n\
         start \"\" \"%~dp0{exe_name}\"\r\n\
         pause\r\n\
         exit /b 1\r\n\
         :rollback_failed\r\n\
         echo 更新失败且自动恢复失败。旧版本备份位于：%~dp0{backup_name}\r\n\
         pause\r\n\
         exit /b 2\r\n"
    );
    fs::write(&script, body).map_err(|e| e.to_string())?;

    Command::new("cmd")
        .args(["/C", "start", "", &script.to_string_lossy()])
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(format!(
        "已下载 {}（SHA256 已校验），即将重启替换。旧版本将保留为 {}。",
        manifest.label(),
        backup_name
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
            url: "https://x/a.exe".into(),
            changelog: String::new(),
            sha256: "a".repeat(64),
        };
        assert!(need_update(&by_code));

        let same = RemoteManifest {
            version_code: APP_VERSION_CODE,
            version_name: APP_VERSION_NAME.to_string(),
            url: "https://x/a.exe".into(),
            changelog: String::new(),
            sha256: "b".repeat(64),
        };
        assert!(!need_update(&same));
    }

    #[test]
    fn parses_jiaoben_style_manifest() {
        let j = format!(
            r#"{{"versionCode":5,"versionName":"0.3.0","desktopUrl":"https://x/a.exe","changelog":"fix","sha256":"{}"}}"#,
            "a".repeat(64)
        );
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        let m = parse_update_payload(&v).unwrap();
        assert_eq!(m.version_code, 5);
        assert_eq!(m.version_name, "0.3.0");
        assert_eq!(m.url, "https://x/a.exe");
        assert_eq!(m.sha256, "a".repeat(64));
    }

    #[test]
    fn parses_wrapped_data() {
        let j = r#"{"code":200,"data":{"versionCode":2,"versionName":"0.2.0","desktopUrl":"https://y/b.exe"}}"#;
        let v: serde_json::Value = serde_json::from_str(j).unwrap();
        let payload = &v["data"];
        let m = parse_update_payload(payload).unwrap();
        assert_eq!(m.version_code, 2);
        assert_eq!(m.url, "https://y/b.exe");
    }

    #[test]
    fn candidate_urls_match_reader_shape() {
        let u = candidate_urls("https://1ph1hf8043323.vicp.fun");
        assert_eq!(
            u[0],
            "https://1ph1hf8043323.vicp.fun/api/app-update/disk-janitor"
        );
        assert_eq!(
            u[1],
            "https://1ph1hf8043323.vicp.fun/disk-janitor/app-update.json"
        );
        assert_eq!(
            u[2],
            "https://1ph1hf8043323.vicp.fun/api/disk-janitor/app-update"
        );
    }

    #[test]
    fn rewrite_old_ip_to_vicp_fun() {
        assert_eq!(
            rewrite_public_host("http://111.229.202.251:8687"),
            DEFAULT_API_BASE
        );
        assert_eq!(
            rewrite_public_host("http://1ph1hf8043323.vicp.fun:8687/dl/a.exe"),
            "https://1ph1hf8043323.vicp.fun/dl/a.exe"
        );
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

    #[test]
    fn validates_https_url_and_sha256() {
        assert!(validate_https_url("https://example.test/update.exe").is_ok());
        assert!(validate_https_url("HTTPS://example.test/update.exe").is_ok());
        assert!(validate_https_url("http://example.test/update.exe").is_err());
        assert!(validate_https_url("https://").is_err());
        assert!(validate_sha256(&"aF".repeat(32)).is_ok());
        assert!(validate_sha256(&"a".repeat(63)).is_err());
        assert!(validate_sha256(&"g".repeat(64)).is_err());
    }

    #[test]
    fn validates_manifest_with_optional_signature_verifier() {
        struct Reject;
        impl ManifestSignatureVerifier for Reject {
            fn verify(&self, _: &RemoteManifest) -> Result<(), String> {
                Err("bad signature".into())
            }
        }
        let manifest = RemoteManifest {
            version_code: 1,
            version_name: "1.0.0".into(),
            url: "https://example.test/update.exe".into(),
            changelog: String::new(),
            sha256: "c".repeat(64),
        };
        assert!(validate_remote_manifest(&manifest, None).is_ok());
        assert!(validate_remote_manifest(&manifest, Some(&Reject)).is_err());
    }

    #[test]
    fn enforces_download_size_limit_logic() {
        assert_eq!(
            checked_download_size(MAX_UPDATE_BYTES - 1, 1).unwrap(),
            MAX_UPDATE_BYTES
        );
        assert!(checked_download_size(MAX_UPDATE_BYTES, 1).is_err());
        assert!(validate_content_length(MAX_UPDATE_BYTES).is_ok());
        assert!(validate_content_length(MAX_UPDATE_BYTES + 1).is_err());
        let mut source = std::io::Cursor::new(vec![1u8; 1024]);
        let mut target = Vec::new();
        assert_eq!(write_limited(&mut source, &mut target).unwrap(), 1024);
        assert_eq!(target.len(), 1024);
    }

    #[test]
    fn accepts_only_expected_binary_content_types() {
        assert!(is_allowed_binary_content_type(Some(
            "application/octet-stream; charset=binary"
        )));
        assert!(is_allowed_binary_content_type(Some(
            "application/vnd.microsoft.portable-executable"
        )));
        assert!(!is_allowed_binary_content_type(Some("text/html")));
        assert!(!is_allowed_binary_content_type(None));
    }
}
