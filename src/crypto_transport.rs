//! run-jane 敏感接口的加密信封（与 Python 端 `crypto_transport.py` 同协议）。
//!
//! - 请求：RSA-OAEP(SHA-256/MGF1-SHA-256) 包裹随机 AES-256 key，业务 JSON 用 AES-256-GCM；
//! - AAD 绑定 `version|keyId|timestamp|requestId|scope`（响应再加 `|response`）；
//! - 响应：同一 AES key 解密（服务端返回的信封）；
//! - keyId 失效（服务端轮换公钥）时刷新公钥并只重试一次。
//!
//! 明文/密文/服务端原始响应都不写日志。

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand_core::{OsRng, RngCore};
use rsa::pkcs8::DecodePublicKey;
use rsa::{Oaep, RsaPublicKey};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::Duration;

#[derive(Clone)]
struct PublicKeyInfo {
    version: String,
    key_id: String,
    key: RsaPublicKey,
}

static KEY_CACHE: Mutex<Option<(String, PublicKeyInfo)>> = Mutex::new(None);

/// 取 `scheme://host` 前缀，用于拼公钥地址。
fn base_of(url: &str) -> String {
    if let Some(pos) = url.find("://") {
        let rest = &url[pos + 3..];
        if let Some(slash) = rest.find('/') {
            return format!("{}{}", &url[..pos + 3], &rest[..slash]);
        }
    }
    url.trim_end_matches('/').to_string()
}

fn agent(timeout: f64) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs_f64(timeout.max(1.0)))
        .build()
}

fn looks_like_key_error(value: &Value) -> bool {
    let text = value.to_string().to_lowercase();
    [
        "keyid",
        "key_id",
        "key-id",
        "key id",
        "public key",
        "unknown key",
        "invalid key",
        "公钥",
        "密钥",
    ]
    .iter()
    .any(|word| text.contains(word))
}

fn clear_public_key_cache() {
    *KEY_CACHE.lock().unwrap() = None;
}

fn get_public_key(base_url: &str, timeout: f64, force_refresh: bool) -> Result<PublicKeyInfo, String> {
    let url = format!("{}/api/crypto/public-key", base_url.trim_end_matches('/'));
    if !force_refresh {
        if let Some((cached_url, info)) = KEY_CACHE.lock().unwrap().as_ref() {
            if cached_url == &url {
                return Ok(info.clone());
            }
        }
    }
    let text = agent(timeout)
        .get(&url)
        .set("Accept", "application/json")
        .call()
        .map_err(|e| format!("获取公钥失败（{e}）"))?
        .into_string()
        .map_err(|e| format!("获取公钥失败（{e}）"))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|_| "公钥响应格式无效".to_string())?;
    let data = value
        .get("data")
        .filter(|d| d.is_object())
        .ok_or_else(|| "公钥响应格式无效".to_string())?;
    let version = data
        .get("version")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let key_id = data
        .get("keyId")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let encoded = data
        .get("publicKey")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if version.is_empty() || key_id.is_empty() || encoded.is_empty() {
        return Err("公钥响应字段不完整".into());
    }
    let der = B64.decode(&encoded).map_err(|_| "服务端公钥无效".to_string())?;
    let key = RsaPublicKey::from_public_key_der(&der).map_err(|_| "服务端公钥无效".to_string())?;
    let info = PublicKeyInfo {
        version,
        key_id,
        key,
    };
    *KEY_CACHE.lock().unwrap() = Some((url, info.clone()));
    Ok(info)
}

fn build_aad(
    version: &str,
    key_id: &str,
    timestamp: i64,
    request_id: &str,
    scope: &str,
    response: bool,
) -> Vec<u8> {
    let mut text = format!("{version}|{key_id}|{timestamp}|{request_id}|{scope}");
    if response {
        text.push_str("|response");
    }
    text.into_bytes()
}

fn uuid_like() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hexed = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hexed[0..8],
        &hexed[8..12],
        &hexed[12..16],
        &hexed[16..20],
        &hexed[20..32]
    )
}

struct RequestKeys {
    envelope: Value,
    aes_key: [u8; 32],
    timestamp: i64,
    request_id: String,
}

fn build_request_envelope(
    payload: &Value,
    scope: &str,
    info: &PublicKeyInfo,
) -> Result<RequestKeys, String> {
    let mut aes_key = [0u8; 32];
    OsRng.fill_bytes(&mut aes_key);
    let mut iv = [0u8; 12];
    OsRng.fill_bytes(&mut iv);
    let timestamp = chrono::Utc::now().timestamp_millis();
    let request_id = uuid_like();

    let plaintext = serde_json::to_vec(payload).map_err(|_| "请求序列化失败".to_string())?;
    let cipher =
        Aes256Gcm::new_from_slice(&aes_key).map_err(|_| "加密初始化失败".to_string())?;
    let aad = build_aad(
        &info.version,
        &info.key_id,
        timestamp,
        &request_id,
        scope,
        false,
    );
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&iv),
            Payload {
                msg: &plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| "请求加密失败".to_string())?;
    let encrypted_key = info
        .key
        .encrypt(&mut OsRng, Oaep::new::<sha2::Sha256>(), &aes_key)
        .map_err(|_| "信封密钥封装失败".to_string())?;

    Ok(RequestKeys {
        envelope: json!({
            "version": info.version,
            "keyId": info.key_id,
            "timestamp": timestamp,
            "requestId": request_id,
            "scope": scope,
            "iv": B64.encode(iv),
            "encryptedKey": B64.encode(encrypted_key),
            "ciphertext": B64.encode(ciphertext),
        }),
        aes_key,
        timestamp,
        request_id,
    })
}

fn encrypted_part(response: &Value) -> Option<&Value> {
    if response.get("iv").is_some() && response.get("ciphertext").is_some() {
        return Some(response);
    }
    response
        .get("data")
        .filter(|d| d.get("iv").is_some() && d.get("ciphertext").is_some())
}

fn decrypt_response(
    response: &Value,
    keys: &RequestKeys,
    info: &PublicKeyInfo,
    scope: &str,
) -> Result<Value, String> {
    let part = encrypted_part(response).ok_or_else(|| "安全响应格式无效".to_string())?;
    let iv = B64
        .decode(part.get("iv").and_then(|x| x.as_str()).unwrap_or(""))
        .map_err(|_| "安全响应格式无效".to_string())?;
    let ciphertext = B64
        .decode(part.get("ciphertext").and_then(|x| x.as_str()).unwrap_or(""))
        .map_err(|_| "安全响应格式无效".to_string())?;
    let cipher =
        Aes256Gcm::new_from_slice(&keys.aes_key).map_err(|_| "解密初始化失败".to_string())?;
    let aad = build_aad(
        &info.version,
        &info.key_id,
        keys.timestamp,
        &keys.request_id,
        scope,
        true,
    );
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&iv),
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| "安全响应校验失败".to_string())?;
    serde_json::from_slice(&plaintext).map_err(|_| "安全响应格式无效".to_string())
}

/// 向敏感接口发送加密信封，返回**解密后的业务 JSON**（通常为 `{code,message,data}`）。
pub fn secure_json_request(
    url: &str,
    payload: &Value,
    scope: &str,
    timeout: f64,
) -> Result<Value, String> {
    if scope.trim().is_empty() {
        return Err("安全请求 scope 不能为空".into());
    }
    let base = base_of(url);
    for attempt in 0..2 {
        let info = get_public_key(&base, timeout, attempt == 1)?;
        let keys = build_request_envelope(payload, scope, &info)?;
        let raw = serde_json::to_string(&keys.envelope).map_err(|_| "信封序列化失败".to_string())?;
        let response = agent(timeout)
            .post(url)
            .set("Content-Type", "application/json")
            .set("Accept", "application/json")
            .send_string(&raw);
        let text = match response {
            Ok(r) => r
                .into_string()
                .map_err(|e| format!("安全请求失败（{e}）"))?,
            // 4xx 正文可能仍是可解密的信封（例如「卡密不存在」），继续尝试解密
            Err(ureq::Error::Status(_, r)) => r
                .into_string()
                .map_err(|e| format!("安全请求失败（{e}）"))?,
            Err(_) => return Err("安全请求网络失败".into()),
        };
        let value: Value =
            serde_json::from_str(&text).map_err(|_| "安全响应格式无效".to_string())?;
        match decrypt_response(&value, &keys, &info, scope) {
            Ok(decoded) => return Ok(decoded),
            Err(err) => {
                if attempt == 0 && looks_like_key_error(&value) {
                    clear_public_key_cache();
                    continue;
                }
                return Err(err);
            }
        }
    }
    Err("安全请求失败".into())
}
