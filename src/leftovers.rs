//! 卸载残留：以「按应用跟扫」为主；全盘粗扫收窄且默认不勾选

use crate::software::{expand_env, list_installed_apps, InstalledApp};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    pub fn label(self) -> &'static str {
        match self {
            Self::High => "高",
            Self::Medium => "中",
            Self::Low => "低",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LeftoverHit {
    pub path: PathBuf,
    pub reason: String,
    pub size_hint: u64,
    pub selected: bool,
    pub confidence: Confidence,
}

/// 跟扫线索：来自卸载前快照
#[derive(Debug, Clone)]
pub struct AppLeftoverHint {
    pub display_name: String,
    pub publisher: String,
    pub install_location: String,
    pub uninstall_registry_path: String,
}

impl AppLeftoverHint {
    pub fn from_app(app: &InstalledApp) -> Self {
        Self {
            display_name: app.display_name.clone(),
            publisher: app.publisher.clone(),
            install_location: app.install_location.clone(),
            uninstall_registry_path: app.reg_path.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallArtifactKind {
    InstallDirectory,
    AppDataDirectory,
    ProgramDataDirectory,
    RegistryKey,
    Service,
    ScheduledTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactScope {
    CurrentUser,
    AllUsers,
    System,
}

/// 这些字段供 UI 明示风险；快照/差异 API 本身没有删除能力。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactSafety {
    pub evidence_only: bool,
    pub automatic_deletion_allowed: bool,
    pub user_content_possible: bool,
    pub requires_admin: bool,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallArtifact {
    pub kind: UninstallArtifactKind,
    pub scope: ArtifactScope,
    pub identifier: String,
    pub exists: bool,
    pub size_hint: u64,
    pub modified_unix_secs: Option<u64>,
    pub selected: bool,
    pub safety: ArtifactSafety,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UninstallClues {
    /// 必须来自本机已枚举的服务名，不接受模糊匹配结果。
    pub service_names: Vec<String>,
    /// 必须来自本机已枚举的完整任务路径。
    pub scheduled_task_paths: Vec<String>,
    /// 除卸载项注册表路径外，由调用方经本机枚举确认的键。
    pub registry_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct UninstallSnapshot {
    pub app: AppLeftoverHint,
    pub captured_unix_secs: u64,
    pub artifacts: Vec<UninstallArtifact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallDiffStatus {
    Removed,
    Remains,
    Appeared,
    Changed,
}

#[derive(Debug, Clone)]
pub struct UninstallDiffEntry {
    pub status: UninstallDiffStatus,
    pub before: Option<UninstallArtifact>,
    pub after: Option<UninstallArtifact>,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct UninstallDiff {
    pub before_captured_unix_secs: u64,
    pub after_captured_unix_secs: u64,
    pub entries: Vec<UninstallDiffEntry>,
}

/// 捕获卸载前/后的只读证据。路径候选仅由内置根目录与应用名称生成；
/// 注册表、服务、任务只记录调用方已在本机精确枚举到的标识。
pub fn capture_uninstall_snapshot(
    hint: &AppLeftoverHint,
    clues: &UninstallClues,
    cancel: &AtomicBool,
) -> UninstallSnapshot {
    let mut artifacts = Vec::new();
    let mut seen = HashSet::new();
    if !hint.install_location.trim().is_empty() {
        push_path_artifact(
            &mut artifacts,
            &mut seen,
            PathBuf::from(expand_env(hint.install_location.trim())),
            UninstallArtifactKind::InstallDirectory,
            ArtifactScope::System,
            cancel,
        );
    }
    for (path, kind, scope) in snapshot_path_candidates(hint) {
        push_path_artifact(&mut artifacts, &mut seen, path, kind, scope, cancel);
    }
    let mut registry_paths = clues.registry_paths.clone();
    if !hint.uninstall_registry_path.trim().is_empty() {
        registry_paths.push(hint.uninstall_registry_path.clone());
    }
    for path in registry_paths {
        let exists = registry_path_exists(&path);
        push_evidence_artifact(
            &mut artifacts,
            &mut seen,
            UninstallArtifactKind::RegistryKey,
            if path.to_ascii_uppercase().starts_with("HKCU\\") {
                ArtifactScope::CurrentUser
            } else {
                ArtifactScope::System
            },
            path,
            exists,
        );
    }
    for name in &clues.service_names {
        push_evidence_artifact(
            &mut artifacts,
            &mut seen,
            UninstallArtifactKind::Service,
            ArtifactScope::System,
            name.clone(),
            true,
        );
    }
    for path in &clues.scheduled_task_paths {
        push_evidence_artifact(
            &mut artifacts,
            &mut seen,
            UninstallArtifactKind::ScheduledTask,
            ArtifactScope::System,
            path.clone(),
            true,
        );
    }
    artifacts.sort_by(|a, b| artifact_key(a).cmp(&artifact_key(b)));
    UninstallSnapshot {
        app: hint.clone(),
        captured_unix_secs: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        artifacts,
    }
}

pub fn diff_uninstall_snapshots(
    before: &UninstallSnapshot,
    after: &UninstallSnapshot,
) -> UninstallDiff {
    let mut keys = HashSet::new();
    keys.extend(before.artifacts.iter().map(artifact_key));
    keys.extend(after.artifacts.iter().map(artifact_key));
    let mut keys: Vec<_> = keys.into_iter().collect();
    keys.sort();
    let mut entries = Vec::new();
    for key in keys {
        let old = before.artifacts.iter().find(|a| artifact_key(a) == key);
        let new = after.artifacts.iter().find(|a| artifact_key(a) == key);
        let status = match (old, new) {
            (Some(a), Some(b)) if a.exists && !b.exists => UninstallDiffStatus::Removed,
            (Some(a), Some(b)) if !a.exists && b.exists => UninstallDiffStatus::Appeared,
            (Some(a), Some(b))
                if a.exists
                    && b.exists
                    && (a.size_hint != b.size_hint
                        || a.modified_unix_secs != b.modified_unix_secs) =>
            {
                UninstallDiffStatus::Changed
            }
            (Some(_), Some(_)) => UninstallDiffStatus::Remains,
            (Some(a), None) if a.exists => UninstallDiffStatus::Removed,
            (Some(_), None) => UninstallDiffStatus::Remains,
            (None, Some(b)) if b.exists => UninstallDiffStatus::Appeared,
            (None, Some(_)) => UninstallDiffStatus::Remains,
            (None, None) => continue,
        };
        entries.push(UninstallDiffEntry {
            status,
            before: old.cloned(),
            after: new.cloned(),
            selected: false,
        });
    }
    UninstallDiff {
        before_captured_unix_secs: before.captured_unix_secs,
        after_captured_unix_secs: after.captured_unix_secs,
        entries,
    }
}

fn snapshot_path_candidates(
    hint: &AppLeftoverHint,
) -> Vec<(PathBuf, UninstallArtifactKind, ArtifactScope)> {
    let mut names = name_tokens(&hint.display_name);
    names.extend(name_tokens(&hint.publisher));
    names.retain(|name| {
        let count = name.chars().count();
        (!name.is_ascii() && count >= 2) || (name.is_ascii() && count >= 3)
    });
    names.sort();
    names.dedup();
    names.truncate(8);

    let mut out = Vec::new();
    for (env, kind, scope) in [
        (
            "LOCALAPPDATA",
            UninstallArtifactKind::AppDataDirectory,
            ArtifactScope::CurrentUser,
        ),
        (
            "APPDATA",
            UninstallArtifactKind::AppDataDirectory,
            ArtifactScope::CurrentUser,
        ),
        (
            "ProgramData",
            UninstallArtifactKind::ProgramDataDirectory,
            ArtifactScope::AllUsers,
        ),
    ] {
        let Ok(root) = std::env::var(env) else {
            continue;
        };
        for name in &names {
            out.push((PathBuf::from(&root).join(name), kind, scope));
        }
    }
    out
}

fn push_path_artifact(
    artifacts: &mut Vec<UninstallArtifact>,
    seen: &mut HashSet<String>,
    path: PathBuf,
    kind: UninstallArtifactKind,
    scope: ArtifactScope,
    cancel: &AtomicBool,
) {
    let identifier = path.to_string_lossy().to_string();
    let key = format!("{kind:?}|{}", norm(&path));
    if !seen.insert(key) {
        return;
    }
    let metadata = std::fs::symlink_metadata(&path).ok();
    let exists = metadata.is_some();
    let size_hint = if metadata.as_ref().is_some_and(|m| m.is_file()) {
        metadata.as_ref().map_or(0, |m| m.len())
    } else if exists && !cancel.load(Ordering::Relaxed) {
        dir_size_cap(&path, cancel, 20_000)
    } else {
        0
    };
    let modified_unix_secs = metadata
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    artifacts.push(UninstallArtifact {
        kind,
        scope,
        identifier,
        exists,
        size_hint,
        modified_unix_secs,
        selected: false,
        safety: ArtifactSafety {
            evidence_only: true,
            automatic_deletion_allowed: false,
            user_content_possible: true,
            requires_admin: scope != ArtifactScope::CurrentUser,
            rationale: "仅作为卸载前后差异线索；需人工核验归属".into(),
        },
    });
}

fn push_evidence_artifact(
    artifacts: &mut Vec<UninstallArtifact>,
    seen: &mut HashSet<String>,
    kind: UninstallArtifactKind,
    scope: ArtifactScope,
    identifier: String,
    exists: bool,
) {
    let identifier = identifier.trim().to_string();
    if identifier.is_empty() {
        return;
    }
    let key = format!("{kind:?}|{}", identifier.to_ascii_lowercase());
    if !seen.insert(key) {
        return;
    }
    artifacts.push(UninstallArtifact {
        kind,
        scope,
        identifier,
        exists,
        size_hint: 0,
        modified_unix_secs: None,
        selected: false,
        safety: ArtifactSafety {
            evidence_only: true,
            automatic_deletion_allowed: false,
            user_content_possible: false,
            requires_admin: scope != ArtifactScope::CurrentUser,
            rationale: "系统配置线索；不提供自动删除".into(),
        },
    });
}

fn artifact_key(artifact: &UninstallArtifact) -> String {
    format!(
        "{:?}|{}",
        artifact.kind,
        artifact.identifier.to_ascii_lowercase()
    )
}

#[cfg(windows)]
fn registry_path_exists(full_path: &str) -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let normalized = full_path.replace('/', "\\");
    let Some((hive, subkey)) = normalized.split_once('\\') else {
        return false;
    };
    let root = match hive.to_ascii_uppercase().as_str() {
        "HKCU" | "HKEY_CURRENT_USER" => RegKey::predef(HKEY_CURRENT_USER),
        "HKLM" | "HKEY_LOCAL_MACHINE" => RegKey::predef(HKEY_LOCAL_MACHINE),
        _ => return false,
    };
    root.open_subkey(subkey).is_ok()
}

#[cfg(not(windows))]
fn registry_path_exists(_full_path: &str) -> bool {
    false
}

/// 推荐路径：只按某一软件线索跟扫（误报远少于粗扫）
pub fn scan_leftovers_for_app(hint: &AppLeftoverHint, cancel: &AtomicBool) -> Vec<LeftoverHit> {
    let tokens = name_tokens(&hint.display_name);
    let pub_tokens = name_tokens(&hint.publisher);
    let mut hits = Vec::new();
    let mut seen = HashSet::new();

    // 1) 安装目录若仍在 → 高置信
    if !hint.install_location.trim().is_empty() {
        let p = PathBuf::from(expand_env(hint.install_location.trim()));
        if p.is_dir() && !cancel.load(Ordering::Relaxed) {
            let sz = dir_size_cap(&p, cancel, 12_000);
            if sz > 0 {
                push_hit(
                    &mut hits,
                    &mut seen,
                    p,
                    format!("安装目录仍存在（卸载后残留）· {}", hint.display_name),
                    sz,
                    Confidence::High,
                );
            }
        }
    }

    if tokens.is_empty() {
        hits.sort_by(|a, b| b.size_hint.cmp(&a.size_hint));
        return hits;
    }

    // 2) 在收窄根目录下找名称匹配的文件夹
    for root in followup_roots() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        scan_root_for_tokens(&root, &tokens, &pub_tokens, cancel, &mut hits, &mut seen);
    }

    hits.sort_by(|a, b| {
        confidence_rank(b.confidence)
            .cmp(&confidence_rank(a.confidence))
            .then_with(|| b.size_hint.cmp(&a.size_hint))
    });
    hits.truncate(80);
    hits
}

/// 粗扫：仅 Program Files / Local\\Programs，且目录内需有可执行文件迹象；全部默认不勾选
pub fn scan_leftovers(cancel: &AtomicBool) -> Vec<LeftoverHit> {
    let apps = list_installed_apps();
    let mut known = HashSet::new();
    let mut known_names: Vec<String> = Vec::new();
    for a in &apps {
        known_names.push(a.display_name.to_lowercase());
        if !a.install_location.trim().is_empty() {
            let p = PathBuf::from(expand_env(a.install_location.trim()));
            if let Ok(c) = p.canonicalize() {
                known.insert(norm(&c));
            } else {
                known.insert(norm(&p));
            }
        }
    }

    let mut hits = Vec::new();
    let mut seen = HashSet::new();
    for root in broad_roots() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if !root.is_dir() {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&root) else {
            continue;
        };
        for ent in rd.flatten() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let p = ent.path();
            let Ok(ft) = ent.file_type() else {
                continue;
            };
            if !ft.is_dir() {
                continue;
            }
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.is_empty() || is_noise_folder(&name) {
                continue;
            }
            let key = norm(&p);
            if seen.contains(&key) {
                continue;
            }
            if known.iter().any(|k| path_related(k, &key)) {
                continue;
            }
            let name_l = name.to_lowercase();
            // 名称与任一已安装软件明显相关则跳过
            if known_names.iter().any(|dn| names_related(dn, &name_l)) {
                continue;
            }
            // 必须像「软件目录」：顶层或下一层有 .exe/.msi
            if !looks_like_app_folder(&p) {
                continue;
            }
            let size_hint = dir_size_cap(&p, cancel, 6_000);
            if size_hint < 1024 * 1024 {
                continue;
            }
            push_hit(
                &mut hits,
                &mut seen,
                p,
                format!("粗扫候选 · 未匹配卸载项 · {}", root.display()),
                size_hint,
                Confidence::Low,
            );
        }
    }

    hits.sort_by(|a, b| b.size_hint.cmp(&a.size_hint));
    hits.truncate(40);
    hits
}

fn scan_root_for_tokens(
    root: &Path,
    tokens: &[String],
    pub_tokens: &[String],
    cancel: &AtomicBool,
    hits: &mut Vec<LeftoverHit>,
    seen: &mut HashSet<String>,
) {
    if !root.is_dir() {
        return;
    }
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    for ent in rd.flatten() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let p = ent.path();
        if !p.is_dir() {
            continue;
        }
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.is_empty() || is_noise_folder(&name) {
            continue;
        }
        let name_l = name.to_lowercase();
        let token_hit = tokens.iter().any(|t| name_l.contains(t));
        let pub_hit = pub_tokens.iter().any(|t| name_l.contains(t));
        if !token_hit && !pub_hit {
            // 再下一层：仅当根是 Local/Roaming 时扫一层子目录
            if is_appdata_root(root) {
                if let Ok(sub) = std::fs::read_dir(&p) {
                    for s in sub.flatten().take(80) {
                        let sp = s.path();
                        if !sp.is_dir() {
                            continue;
                        }
                        let sn = sp
                            .file_name()
                            .map(|n| n.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        if tokens.iter().any(|t| sn.contains(t)) {
                            let sz = dir_size_cap(&sp, cancel, 8_000);
                            if sz >= 64 * 1024 {
                                let conf = if looks_like_app_folder(&sp) {
                                    Confidence::Medium
                                } else {
                                    Confidence::Low
                                };
                                push_hit(
                                    hits,
                                    seen,
                                    sp,
                                    format!("名称匹配「{}」", tokens.join("/")),
                                    sz,
                                    conf,
                                );
                            }
                        }
                    }
                }
            }
            continue;
        }
        let sz = dir_size_cap(&p, cancel, 10_000);
        if sz < 64 * 1024 {
            continue;
        }
        let conf = if token_hit && looks_like_app_folder(&p) {
            Confidence::High
        } else if token_hit {
            Confidence::Medium
        } else {
            Confidence::Low
        };
        push_hit(
            hits,
            seen,
            p,
            if token_hit {
                format!("名称匹配「{}」", tokens.join("/"))
            } else {
                format!("发布者匹配「{}」", pub_tokens.join("/"))
            },
            sz,
            conf,
        );
    }
}

fn push_hit(
    hits: &mut Vec<LeftoverHit>,
    seen: &mut HashSet<String>,
    path: PathBuf,
    reason: String,
    size_hint: u64,
    confidence: Confidence,
) {
    let key = norm(&path);
    if !seen.insert(key) {
        return;
    }
    hits.push(LeftoverHit {
        path,
        reason,
        size_hint,
        selected: false, // 永远默认不勾选
        confidence,
    });
}

fn confidence_rank(c: Confidence) -> u8 {
    match c {
        Confidence::High => 3,
        Confidence::Medium => 2,
        Confidence::Low => 1,
    }
}

fn followup_roots() -> Vec<PathBuf> {
    let mut v = broad_roots();
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        v.push(PathBuf::from(&local));
        v.push(PathBuf::from(&local).join("Programs"));
    }
    if let Ok(roaming) = std::env::var("APPDATA") {
        v.push(PathBuf::from(roaming));
    }
    if let Ok(pd) = std::env::var("ProgramData") {
        v.push(PathBuf::from(pd));
    }
    v
}

fn broad_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(p) = std::env::var(key) {
            v.push(PathBuf::from(p));
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        v.push(PathBuf::from(local).join("Programs"));
    }
    v
}

fn is_appdata_root(root: &Path) -> bool {
    let s = root.to_string_lossy().to_ascii_lowercase();
    s.ends_with("\\appdata\\local")
        || s.ends_with("\\appdata\\roaming")
        || s.ends_with("\\programdata")
}

fn name_tokens(s: &str) -> Vec<String> {
    let stop = [
        "the",
        "for",
        "and",
        "inc",
        "ltd",
        "llc",
        "corp",
        "co",
        "app",
        "apps",
        "software",
        "tool",
        "tools",
        "version",
        "setup",
        "install",
        "installer",
        "x64",
        "x86",
        "win",
        "windows",
        "microsoft",
        "update",
        "runtime",
        "redistributable",
        "framework",
        "of",
        "to",
        "in",
        "on",
        "by",
        "or",
        "as",
        "at",
        "an",
        "is",
        "it",
        "be",
        "me",
        "my",
        "we",
    ];
    let mut out = Vec::new();
    let mut buf = String::new();
    let flush = |buf: &mut String, out: &mut Vec<String>| {
        if buf.is_empty() {
            return;
        }
        let chars = buf.chars().count();
        let ok = if buf.is_ascii() {
            // 允许 QQ、360 等 2 字符品牌；过滤 of/to 等停用词
            chars >= 3 || (chars == 2 && !stop.contains(&buf.as_str()))
        } else {
            chars >= 2
        };
        if ok && !stop.contains(&buf.as_str()) {
            out.push(buf.clone());
        }
        buf.clear();
    };
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            buf.push(ch.to_ascii_lowercase());
        } else {
            flush(&mut buf, &mut out);
        }
    }
    flush(&mut buf, &mut out);
    // 短名兜底：如 QQ、飞书 被滤光后仍给一个紧凑 token
    if out.is_empty() {
        let compact: String = s
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect();
        if compact.chars().count() >= 2 {
            out.push(compact);
        }
    }
    let mut seen = HashSet::new();
    out.retain(|t| seen.insert(t.clone()));
    out.truncate(6);
    out
}

fn names_related(installed: &str, folder: &str) -> bool {
    if installed.is_empty() || folder.is_empty() {
        return false;
    }
    if installed.contains(folder) || folder.contains(installed) {
        return true;
    }
    let compact_i: String = installed.chars().filter(|c| c.is_alphanumeric()).collect();
    let compact_f: String = folder.chars().filter(|c| c.is_alphanumeric()).collect();
    if compact_i.len() >= 5 && compact_f.len() >= 5 {
        return compact_i.contains(&compact_f) || compact_f.contains(&compact_i);
    }
    false
}

fn path_related(a: &str, b: &str) -> bool {
    a == b || a.starts_with(&format!("{b}\\")) || b.starts_with(&format!("{a}\\"))
}

fn looks_like_app_folder(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    let mut dirs = 0usize;
    for ent in rd.flatten().take(60) {
        let p = ent.path();
        if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            let e = ext.to_ascii_lowercase();
            if e == "exe" || e == "msi" || e == "dll" {
                return true;
            }
        }
        if p.is_dir() {
            dirs += 1;
            if dirs <= 8 {
                if let Ok(sub) = std::fs::read_dir(&p) {
                    for s in sub.flatten().take(30) {
                        if let Some(ext) = s.path().extension().and_then(|e| e.to_str()) {
                            let e = ext.to_ascii_lowercase();
                            if e == "exe" || e == "msi" {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_noise_folder(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    matches!(
        l.as_str(),
        "windows"
            | "microsoft"
            | "common files"
            | "internet explorer"
            | "windowsdefender"
            | "temp"
            | "packages"
            | "modifiablewindowsapps"
            | "windowsapps"
            | "dotnet"
            | "reference assemblies"
            | "application data"
            | "history"
            | "temporary internet files"
            | "microsoft shared"
            | "installshield installation information"
    ) || l.starts_with("windows ")
        || l.starts_with("{")
}

fn norm(p: &Path) -> String {
    p.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
}

fn dir_size_cap(path: &Path, cancel: &AtomicBool, max_files: u64) -> u64 {
    let mut total = 0u64;
    let mut files = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) || files >= max_files {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            if files >= max_files {
                break;
            }
            let p = ent.path();
            let Ok(meta) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(p);
            } else {
                total += meta.len();
                files += 1;
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_skip_noise() {
        let t = name_tokens("FooBar Editor for Windows x64");
        assert!(t.iter().any(|x| x == "foobar" || x == "editor"));
        assert!(!t.iter().any(|x| x == "windows" || x == "x64" || x == "for"));
    }

    #[test]
    fn tokens_short_names() {
        let t = name_tokens("QQ");
        assert_eq!(t, vec!["qq".to_string()]);
        let t2 = name_tokens("微信");
        assert!(!t2.is_empty());
        let t3 = name_tokens("QQ Music");
        assert!(t3.iter().any(|x| x == "qq"));
        assert!(t3.iter().any(|x| x == "music"));
    }

    #[test]
    fn scan_broad_does_not_panic() {
        let cancel = AtomicBool::new(false);
        let _ = scan_leftovers(&cancel);
    }

    #[test]
    fn followup_with_empty_hint() {
        let cancel = AtomicBool::new(false);
        let hint = AppLeftoverHint {
            display_name: "Z".into(),
            publisher: String::new(),
            install_location: String::new(),
            uninstall_registry_path: String::new(),
        };
        let hits = scan_leftovers_for_app(&hint, &cancel);
        assert!(hits.is_empty());
    }

    fn test_artifact(identifier: &str, exists: bool, size_hint: u64) -> UninstallArtifact {
        UninstallArtifact {
            kind: UninstallArtifactKind::AppDataDirectory,
            scope: ArtifactScope::CurrentUser,
            identifier: identifier.into(),
            exists,
            size_hint,
            modified_unix_secs: None,
            selected: false,
            safety: ArtifactSafety {
                evidence_only: true,
                automatic_deletion_allowed: false,
                user_content_possible: true,
                requires_admin: false,
                rationale: "test".into(),
            },
        }
    }

    fn test_snapshot(artifacts: Vec<UninstallArtifact>, captured: u64) -> UninstallSnapshot {
        UninstallSnapshot {
            app: AppLeftoverHint {
                display_name: "Example Editor".into(),
                publisher: "Example Corp".into(),
                install_location: String::new(),
                uninstall_registry_path: String::new(),
            },
            captured_unix_secs: captured,
            artifacts,
        }
    }

    #[test]
    fn uninstall_diff_classifies_removed_changed_and_appeared() {
        let before = test_snapshot(
            vec![
                test_artifact("removed", true, 10),
                test_artifact("changed", true, 10),
                test_artifact("appeared", false, 0),
            ],
            1,
        );
        let after = test_snapshot(
            vec![
                test_artifact("removed", false, 0),
                test_artifact("changed", true, 20),
                test_artifact("appeared", true, 5),
            ],
            2,
        );
        let diff = diff_uninstall_snapshots(&before, &after);
        assert!(diff
            .entries
            .iter()
            .any(|entry| entry.status == UninstallDiffStatus::Removed));
        assert!(diff
            .entries
            .iter()
            .any(|entry| entry.status == UninstallDiffStatus::Changed));
        assert!(diff
            .entries
            .iter()
            .any(|entry| entry.status == UninstallDiffStatus::Appeared));
        assert!(diff.entries.iter().all(|entry| !entry.selected));
    }

    #[test]
    fn snapshot_evidence_is_never_auto_deletable_or_selected() {
        let cancel = AtomicBool::new(false);
        let hint = AppLeftoverHint {
            display_name: "Example Editor".into(),
            publisher: "Example Corp".into(),
            install_location: String::new(),
            uninstall_registry_path: r"HKCU\Software\Missing\Example".into(),
        };
        let clues = UninstallClues {
            service_names: vec!["ExampleService".into()],
            scheduled_task_paths: vec![r"\Example\Update".into()],
            registry_paths: Vec::new(),
        };
        let snapshot = capture_uninstall_snapshot(&hint, &clues, &cancel);
        assert!(snapshot.artifacts.iter().all(|artifact| {
            !artifact.selected
                && artifact.safety.evidence_only
                && !artifact.safety.automatic_deletion_allowed
        }));
        assert!(snapshot
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == UninstallArtifactKind::Service));
        assert!(snapshot
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == UninstallArtifactKind::ScheduledTask));
        assert!(snapshot
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == UninstallArtifactKind::RegistryKey));
    }
}
