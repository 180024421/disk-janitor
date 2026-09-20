//! 卡密授权：调用 jiaoben `/api/app-license/disk-janitor/*`，Ed25519 ticket v2 验签。

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub const APP_KEY: &str = "disk-janitor";
/// Ed25519 SPKI base64url — 与 desk-reader / jiaoben APP_LICENSE_TICKET_PUBLIC_KEY 一致
pub const TICKET_PUBLIC_KEY_B64URL: &str =
    "MCowBQYDK2VwAyEAPuGiGKcy19RYif-ir-fhnK5Gr9u3vwQ2SZ148GvIaaI";
pub const LICENSE_API_BASE: &str = "https://jiaoben.lidashuai.top";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LicenseCache {
    #[serde(default)]
    pub ticket: String,
    #[serde(default)]
    pub ticket_expire_at: i64,
    #[serde(default)]
    pub paid_last_seen_at: i64,
    /// paid_last_seen_at 的完整性摘要（HMAC）。明文 JSON 时代可直接改锚点配合
    /// 时钟回拨让过期票据复活；篡改即视为回拨攻击，强制联网。
    #[serde(default)]
    pub guard: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatus {
    pub valid: bool,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub expire_at: Option<String>,
    #[serde(default)]
    pub time_unlimited: bool,
    #[serde(default)]
    pub ticket: Option<String>,
    #[serde(default)]
    pub ticket_expire_at: Option<i64>,
    #[serde(default)]
    pub plan_label: Option<String>,
    #[serde(default)]
    pub device_count: Option<i32>,
    #[serde(default)]
    pub max_devices: Option<i32>,
    #[serde(default)]
    pub from_cache: bool,
}

#[derive(Deserialize)]
struct ApiEnvelope {
    code: Option<i32>,
    message: Option<String>,
    data: Option<LicenseStatus>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn cache_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("disk-janitor").join("license.json")
}

const GUARD_PEPPER: &str = "dj-license-guard-v1";

fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const B: usize = 64;
    let mut k = [0u8; B];
    if key.len() > B {
        let d = Sha256::digest(key);
        k[..32].copy_from_slice(&d);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; B];
    let mut opad = [0x5cu8; B];
    for i in 0..B {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let inner = {
        let mut h = Sha256::new();
        h.update(ipad);
        h.update(msg);
        h.finalize()
    };
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner);
    outer.finalize().into()
}

fn cache_guard(cache: &LicenseCache) -> String {
    // 盐绑定设备指纹：整份 license.json 拷到别的机器也无法通过。
    let key = format!(
        "{GUARD_PEPPER}|{}",
        device_fingerprint()
    );
    let msg = format!(
        "{}|{}|{}",
        cache.paid_last_seen_at, cache.ticket_expire_at, cache.ticket
    );
    hex::encode(hmac_sha256(key.as_bytes(), msg.as_bytes()))
}

pub fn load_cache() -> LicenseCache {
    let p = cache_path();
    let cache: LicenseCache = fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if cache.ticket.is_empty() && cache.paid_last_seen_at == 0 {
        return cache;
    }
    if cache.guard.is_empty() {
        // 0.9.7 升级兼容：旧档没有 guard 字段，采信锚点（钳到不高于当前时间）并补写。
        let mut legacy = cache.clone();
        let now = now_ms();
        if legacy.paid_last_seen_at > now {
            legacy.paid_last_seen_at = now;
        }
        save_cache(&legacy);
        return legacy;
    }
    if cache.guard != cache_guard(&cache) {
        // 锚点被手改/文件被拷贝：按时间攻击处理，清档强制联网复核。
        clear_cache();
        return LicenseCache::default();
    }
    cache
}

pub fn save_cache(cache: &LicenseCache) {
    let p = cache_path();
    let mut fixed = cache.clone();
    fixed.guard = cache_guard(&fixed);
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    // 原子写，避免半写坏档被当成篡改。
    let _ = crate::persistence::save_json(&p, &fixed, false);
}

pub fn clear_cache() {
    let _ = fs::remove_file(cache_path());
}

/// 稳定设备指纹（客户端原始值；服务端会再加 pepper 哈希）
pub fn device_fingerprint() -> String {
    let machine = win_machine_guid().unwrap_or_else(|| "unknown-machine".into());
    let host = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "host".into());
    let user = whoami::username();
    let material = format!("disk-janitor|{machine}|{host}|{user}");
    let digest = Sha256::digest(material.as_bytes());
    hex::encode(digest)
}

pub fn device_name() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "Windows PC".into())
}

fn win_machine_guid() -> Option<String> {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let key = hklm
            .open_subkey("SOFTWARE\\Microsoft\\Cryptography")
            .ok()?;
        key.get_value::<String, _>("MachineGuid").ok()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn b64url_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut t = s.replace('-', "+").replace('_', "/");
    while t.len() % 4 != 0 {
        t.push('=');
    }
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, t)
        .map_err(|e| e.to_string())
}

fn verifying_key() -> Result<VerifyingKey, String> {
    let der = b64url_decode(TICKET_PUBLIC_KEY_B64URL)?;
    if der.len() < 32 {
        return Err("公钥过短".into());
    }
    let raw: [u8; 32] = der[der.len() - 32..]
        .try_into()
        .map_err(|_| "公钥截取失败".to_string())?;
    VerifyingKey::from_bytes(&raw).map_err(|e| e.to_string())
}

/// 验签 ticket；成功返回 ticketExpireAt（ms）
pub fn verify_ticket(ticket: &str, raw_fp: &str) -> Result<i64, String> {
    let (payload_b64, sig_b64) = ticket
        .split_once('.')
        .ok_or_else(|| "票据格式无效".to_string())?;
    let payload = b64url_decode(payload_b64)?;
    let sig_bytes = b64url_decode(sig_b64)?;
    let sig = Signature::from_slice(&sig_bytes).map_err(|e| e.to_string())?;
    let vk = verifying_key()?;
    vk.verify(&payload, &sig)
        .map_err(|_| "票据验签失败".to_string())?;

    let v: serde_json::Value =
        serde_json::from_slice(&payload).map_err(|e| format!("票据 JSON: {e}"))?;
    let version = v.get("v").and_then(|x| x.as_i64()).unwrap_or(1);
    if version < 2 {
        return Err("票据版本过旧".into());
    }
    let app = v.get("a").and_then(|x| x.as_str()).unwrap_or("");
    let raw = v.get("r").and_then(|x| x.as_str()).unwrap_or("");
    if app != APP_KEY || raw != raw_fp {
        return Err("票据与本机不匹配".into());
    }
    let exp = v
        .get("exp")
        .and_then(|x| x.as_i64())
        .ok_or_else(|| "缺少 exp".to_string())?;
    if exp <= now_ms() {
        return Err("票据已过期".into());
    }
    let unlimited = v.get("u").and_then(|x| x.as_i64()).unwrap_or(0) == 1;
    if !unlimited {
        if let Some(e) = v.get("e") {
            if !e.is_null() {
                if let Some(lic_exp) = e.as_i64() {
                    if lic_exp <= now_ms() {
                        return Err("授权已过期".into());
                    }
                }
            }
        }
    }
    Ok(exp)
}

fn guard_clock(cache: &mut LicenseCache) -> Result<(), String> {
    let now = now_ms();
    let prev = cache.paid_last_seen_at;
    // 锚点已有 HMAC 防手改，阈值从 2 小时收紧到 30 分钟：
    // 只容忍时区/夏令时/NTP 步进级别的抖动，不再容忍“拨回 1 小时续命”。
    const ROLLBACK_MS: i64 = 30 * 60 * 1000;
    if prev > 0 && now + 60_000 < prev - ROLLBACK_MS {
        clear_cache();
        return Err("检测到系统时间异常，授权已锁定".into());
    }
    if prev <= 0 || now >= prev {
        cache.paid_last_seen_at = now;
        save_cache(cache);
    }
    Ok(())
}

pub fn read_cached_status() -> Option<LicenseStatus> {
    let mut cache = load_cache();
    if cache.ticket.trim().is_empty() {
        return None;
    }
    let fp = device_fingerprint();
    match verify_ticket(cache.ticket.trim(), &fp) {
        Ok(exp) => {
            if let Err(msg) = guard_clock(&mut cache) {
                return Some(LicenseStatus {
                    valid: false,
                    message: Some(msg),
                    expire_at: None,
                    time_unlimited: false,
                    ticket: None,
                    ticket_expire_at: None,
                    plan_label: None,
                    device_count: None,
                    max_devices: None,
                    from_cache: true,
                });
            }
            Some(LicenseStatus {
                valid: true,
                message: Some("使用已缓存授权（离线宽限）".into()),
                expire_at: None,
                time_unlimited: false,
                ticket: Some(cache.ticket.clone()),
                ticket_expire_at: Some(exp),
                plan_label: None,
                device_count: None,
                max_devices: None,
                from_cache: true,
            })
        }
        Err(_) => {
            clear_cache();
            None
        }
    }
}

fn friendly_license_message(raw: &str) -> String {
    let msg = raw.trim();
    if msg.is_empty() {
        return "操作失败，请稍后重试".into();
    }
    if msg.contains("设备数") || msg.contains("已达上限") || msg.contains("请先解绑") {
        return "该卡密已绑定满 3 台设备。请在旧设备解绑本机后再激活，或联系客服。".into();
    }
    if msg.contains("不存在") || msg.contains("填写有误") {
        return "卡密不存在或填写有误，请核对发货消息中的卡密是否完整。".into();
    }
    if msg.contains("已被使用") {
        return "该卡密已被使用。续费请购买新卡密；换机请用原卡密在未满席设备上激活。".into();
    }
    if msg.contains("不匹配") {
        return "卡密与当前产品不匹配，请确认购买的是「大帅清理器」卡密。".into();
    }
    if msg.contains("时间") || msg.contains("回拨") {
        return "系统时间异常或曾回拨，请校准时间后重试。".into();
    }
    if msg.to_lowercase().contains("timeout")
        || msg.to_lowercase().contains("connection")
        || msg.contains("网络")
        || msg.contains("许可服务")
    {
        return "无法连接授权服务器，请检查网络后重试。".into();
    }
    msg.to_string()
}

fn unwrap_status(body: &str) -> Result<LicenseStatus, String> {
    let env: ApiEnvelope =
        serde_json::from_str(body).map_err(|e| format!("响应解析失败: {e}"))?;
    if let Some(code) = env.code {
        if code >= 400 {
            let raw = env
                .message
                .unwrap_or_else(|| format!("HTTP 业务错误 {code}"));
            return Err(friendly_license_message(&raw));
        }
    }
    if let Some(d) = env.data {
        return Ok(normalize_status(d));
    }
    // try camelCase flat via Value
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("响应解析失败: {e}"))?;
    let raw = if v.get("data").is_some() {
        v.get("data").cloned().unwrap_or(v)
    } else {
        v
    };
    Ok(LicenseStatus {
        valid: raw.get("valid").and_then(|x| x.as_bool()).unwrap_or(false),
        message: raw
            .get("message")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        expire_at: raw
            .get("expireAt")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        time_unlimited: raw
            .get("timeUnlimited")
            .and_then(|x| x.as_bool())
            .unwrap_or(false),
        ticket: raw
            .get("ticket")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        ticket_expire_at: raw.get("ticketExpireAt").and_then(|x| x.as_i64()),
        plan_label: raw
            .get("planLabel")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        device_count: raw
            .get("deviceCount")
            .and_then(|x| x.as_i64())
            .map(|n| n as i32),
        max_devices: raw
            .get("maxDevices")
            .and_then(|x| x.as_i64())
            .map(|n| n as i32),
        from_cache: false,
    })
}

fn normalize_status(mut d: LicenseStatus) -> LicenseStatus {
    d.from_cache = false;
    d
}

/// 明文 POST（仅用于旧服务端兼容）。返回响应正文，4xx 也按业务响应处理。
fn post_plain(url: &str, body: &serde_json::Value) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(20))
        .build();
    match agent
        .post(url)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .send_json(body.clone())
    {
        Ok(resp) => resp
            .into_string()
            .map_err(|e| friendly_license_message(&e.to_string())),
        Err(ureq::Error::Status(_, resp)) => Ok(resp.into_string().unwrap_or_default()),
        Err(e) => Err(friendly_license_message(&e.to_string())),
    }
}

fn post_action(action: &str, card_code: Option<&str>) -> Result<LicenseStatus, String> {
    let fp = device_fingerprint();
    let mut body = serde_json::json!({
        "deviceFingerprint": fp,
        "deviceName": device_name(),
    });
    if let Some(code) = card_code {
        body["cardCode"] = serde_json::Value::String(code.to_string());
    }
    let cache = load_cache();
    if !cache.ticket.trim().is_empty() {
        body["ticket"] = serde_json::Value::String(cache.ticket.clone());
    }
    let url = format!(
        "{}/api/app-license/{APP_KEY}/{action}",
        LICENSE_API_BASE.trim_end_matches('/')
    );
    // 线上敏感接口已强制「加密信封」；旧服务端（或内网自建）才走明文，因此信封失败时回退一次。
    let scope = format!("app-license.{action}");
    let text = match crate::crypto_transport::secure_json_request(&url, &body, &scope, 20.0) {
        Ok(value) => value.to_string(),
        Err(envelope_error) => {
            let plain = post_plain(&url, &body)?;
            // 明文同样被服务端拒绝时，信封侧的错误更有指导意义
            if plain.contains("无效的加密信封") {
                return Err(envelope_error);
            }
            plain
        }
    };
    let status = unwrap_status(&text)?;
    if action == "unbind" {
        clear_cache();
        return Ok(status);
    }
    if status.valid {
        let ticket = status
            .ticket
            .clone()
            .unwrap_or_default();
        let ticket = ticket.trim().to_string();
        if ticket.is_empty() {
            return Err("许可服务器未返回可验签票据".into());
        }
        let exp = verify_ticket(&ticket, &fp)?;
        let mut cache = LicenseCache {
            ticket: ticket.clone(),
            ticket_expire_at: exp,
            paid_last_seen_at: now_ms(),
            guard: String::new(), // save_cache 会统一补算
        };
        guard_clock(&mut cache)?;
        save_cache(&cache);
        let mut out = status;
        out.ticket = Some(ticket);
        out.ticket_expire_at = Some(exp);
        return Ok(out);
    }
    clear_cache();
    Ok(status)
}

pub fn fetch_status() -> Result<LicenseStatus, String> {
    match post_action("status", None) {
        Ok(s) => Ok(s),
        Err(e) => {
            if let Some(cached) = read_cached_status() {
                if cached.valid {
                    return Ok(cached);
                }
            }
            Err(e)
        }
    }
}

pub fn redeem(card_code: &str) -> Result<LicenseStatus, String> {
    let code = card_code.split_whitespace().collect::<Vec<_>>().join(" ");
    if code.is_empty() {
        return Err("请输入卡密".into());
    }
    post_action("redeem", Some(&code))
}

pub fn unbind() -> Result<LicenseStatus, String> {
    post_action("unbind", None)
}

/// 启动时判定是否已解锁
pub fn is_unlocked() -> bool {
    if let Some(s) = read_cached_status() {
        if s.valid {
            return true;
        }
    }
    match fetch_status() {
        Ok(s) => s.valid,
        Err(_) => false,
    }
}

#[cfg(test)]
mod live_probe {
    /// 真机探测（联网，默认不跑）：
    /// `cargo test --release --bin disk-janitor live_status_probe -- --ignored --nocapture`
    ///
    /// 用于确认「加密信封」是否被线上服务端接受：能拿到 `valid`/业务 message 即说明
    /// 信封协议通；若报「无效的加密信封」则说明仍是明文请求。
    #[test]
    #[ignore]
    fn live_status_probe() {
        match super::fetch_status() {
            Ok(s) => println!(
                "PROBE_OK valid={} plan={:?} message={:?}",
                s.valid, s.plan_label, s.message
            ),
            Err(e) => println!("PROBE_ERR {e}"),
        }
    }

    /// 真机兑卡（联网 + 消耗一张真实卡密，默认不跑）：
    /// `$env:DISKJANITOR_TEST_CARD="卡密"; cargo test --release --bin disk-janitor live_redeem_probe -- --ignored --nocapture`
    ///
    /// 成功会验签票据并写入 `%LOCALAPPDATA%\disk-janitor\license.json`（与 exe 共用），
    /// 之后直接启动客户端即可进入主界面。
    #[test]
    #[ignore]
    fn live_redeem_probe() {
        let card = std::env::var("DISKJANITOR_TEST_CARD").unwrap_or_default();
        if card.trim().is_empty() {
            println!("REDEEM_SKIP 未设置 DISKJANITOR_TEST_CARD");
            return;
        }
        match super::redeem(card.trim()) {
            Ok(s) => println!(
                "REDEEM_OK valid={} plan={:?} message={:?} ticket_len={}",
                s.valid,
                s.plan_label,
                s.message,
                s.ticket.as_ref().map(|t| t.len()).unwrap_or(0)
            ),
            Err(e) => println!("REDEEM_ERR {e}"),
        }
    }

    /// 授权失效踢出探测：先兑卡，再由外部把服务端 license 冻结，设 DISKJANITOR_EXPECT_INVALID=1 后重跑。
    #[test]
    #[ignore]
    fn live_expire_kick_probe() {
        let expect_invalid =
            std::env::var("DISKJANITOR_EXPECT_INVALID").ok().as_deref() == Some("1");
        if !expect_invalid {
            let before = match super::fetch_status() {
                Ok(s) => s,
                Err(e) => {
                    println!("EXPIRE_SKIP 当前未激活: {e}");
                    return;
                }
            };
            if !before.valid {
                println!("EXPIRE_SKIP 当前授权无效，先兑卡再测");
                return;
            }
            println!(
                "EXPIRE_BEFORE valid=true plan={:?} msg={:?}",
                before.plan_label, before.message
            );
            println!(
                "EXPIRE_HINT 请先在服务端冻结本机 license，再设 DISKJANITOR_EXPECT_INVALID=1 重跑"
            );
            return;
        }

        // 冻结后本地缓存可能仍短暂有效；在线 status 必须立刻失效并清缓存。
        match super::fetch_status() {
            Ok(s) => {
                println!("EXPIRE_AFTER valid={} msg={:?}", s.valid, s.message);
                assert!(!s.valid, "期望授权失效但 valid=true");
                let cached = super::read_cached_status();
                println!("EXPIRE_CACHE {:?}", cached.as_ref().map(|c| c.valid));
                assert!(
                    cached.as_ref().map(|c| c.valid) != Some(true),
                    "失效后本地缓存仍有效"
                );
                println!("EXPIRE_KICK_OK");
            }
            Err(e) => {
                println!("EXPIRE_AFTER_ERR {e}");
                assert!(
                    e.contains("过期")
                        || e.contains("冻结")
                        || e.contains("失效")
                        || e.contains("授权"),
                    "unexpected err: {e}"
                );
                println!("EXPIRE_KICK_OK_VIA_ERR");
            }
        }
    }
}
