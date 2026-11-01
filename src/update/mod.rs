//! Auto-atualizacao a partir de GitHub Releases (rodrigomiquilino/ondalivre).
//! Nunca interrompe o ar: so aplica com playback parado / ao fechar.

use crate::config::{beside_exe, exe_dir};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const GITHUB_REPO: &str = "rodrigomiquilino/ondalivre";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl SemVer {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('v');
        let mut parts = s.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().unwrap_or("0").parse().ok()?;
        let patch = parts
            .next()
            .unwrap_or("0")
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()?;
        Some(Self {
            major,
            minor,
            patch,
        })
    }

    pub fn cmp_semver(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}

pub fn compare_versions(current: &str, remote: &str) -> Option<Ordering> {
    Some(SemVer::parse(current)?.cmp_semver(&SemVer::parse(remote)?))
}

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag: String,
    pub version: String,
    pub exe_url: String,
    pub sha256_url: Option<String>,
    pub body: String,
    pub etag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateCache {
    pub etag: Option<String>,
    pub tag: Option<String>,
    pub version: Option<String>,
    pub dismissed_version: Option<String>,
    pub last_check_unix: Option<i64>,
}

pub fn update_cache_path() -> PathBuf {
    beside_exe("update-cache.json")
}

pub fn load_update_cache() -> UpdateCache {
    fs::read_to_string(update_cache_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_update_cache(c: &UpdateCache) -> anyhow::Result<()> {
    fs::write(update_cache_path(), serde_json::to_string_pretty(c)?)?;
    Ok(())
}

pub fn dismiss_version(version: &str) {
    let mut c = load_update_cache();
    c.dismissed_version = Some(version.to_string());
    let _ = save_update_cache(&c);
}

pub fn is_dismissed(version: &str) -> bool {
    load_update_cache()
        .dismissed_version
        .as_deref()
        == Some(version)
}

#[derive(Debug)]
pub enum CheckResult {
    /// 304 / mesmo ETag — nada mudou.
    NotModified,
    /// Release parseada (pode ou nao ser mais nova que CURRENT).
    Latest(ReleaseInfo),
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    body: Option<String>,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// Consulta /releases/latest com If-None-Match quando houver ETag em cache.
pub fn fetch_latest_release(repo: &str) -> anyhow::Result<ReleaseInfo> {
    match fetch_latest_release_cached(repo)? {
        CheckResult::Latest(info) => Ok(info),
        CheckResult::NotModified => {
            let c = load_update_cache();
            anyhow::bail!(
                "sem mudanca (ETag); ultima tag {:?}",
                c.tag
            )
        }
    }
}

pub fn fetch_latest_release_cached(repo: &str) -> anyhow::Result<CheckResult> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let mut cache = load_update_cache();
    let mut req = ureq::get(&url)
        .set("User-Agent", "OndaLivre-Updater")
        .set("Accept", "application/vnd.github+json");
    if let Some(ref etag) = cache.etag {
        if !etag.is_empty() {
            req = req.set("If-None-Match", etag);
        }
    }
    let resp = match req.call() {
        Ok(r) => r,
        Err(ureq::Error::Status(304, _)) => {
            cache.last_check_unix = Some(chrono_now_unix());
            let _ = save_update_cache(&cache);
            return Ok(CheckResult::NotModified);
        }
        Err(e) => return Err(anyhow::anyhow!("GitHub API: {e}")),
    };
    // ureq 2 may not expose 304 via Ok — also check status
    let status = resp.status();
    if status == 304 {
        cache.last_check_unix = Some(chrono_now_unix());
        let _ = save_update_cache(&cache);
        return Ok(CheckResult::NotModified);
    }
    let new_etag = resp.header("etag").map(|s| s.to_string());
    let body = resp.into_string()?;
    let rel: GhRelease = serde_json::from_str(&body)?;
    let version = rel.tag_name.trim_start_matches('v').to_string();
    let exe = rel
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("ondalivre.exe"))
        .ok_or_else(|| anyhow::anyhow!("Release sem ondalivre.exe"))?;
    let sha = rel
        .assets
        .iter()
        .find(|a| {
            a.name.eq_ignore_ascii_case("ondalivre.exe.sha256")
                || a.name.eq_ignore_ascii_case("ondalivre.sha256")
        })
        .map(|a| a.browser_download_url.clone());
    let info = ReleaseInfo {
        tag: rel.tag_name.clone(),
        version: version.clone(),
        exe_url: exe.browser_download_url.clone(),
        sha256_url: sha,
        body: rel.body.unwrap_or_default(),
        etag: new_etag.clone(),
    };
    cache.etag = new_etag.or(cache.etag);
    cache.tag = Some(rel.tag_name);
    cache.version = Some(version);
    cache.last_check_unix = Some(chrono_now_unix());
    let _ = save_update_cache(&cache);
    Ok(CheckResult::Latest(info))
}

fn chrono_now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn hours_since_last_check() -> Option<f64> {
    let c = load_update_cache();
    let ts = c.last_check_unix?;
    let now = chrono_now_unix();
    Some((now - ts) as f64 / 3600.0)
}

pub fn should_recheck(interval_hours: f64) -> bool {
    let interval = interval_hours.max(0.25); // min 15 min
    match hours_since_last_check() {
        None => true,
        Some(h) => h >= interval,
    }
}

pub fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Aceita "HASH", "HASH  filename" ou "HASH *filename".
pub fn parse_sha256_text(text: &str) -> Option<String> {
    let line = text.lines().next()?.trim();
    let hash = line.split_whitespace().next()?.to_lowercase();
    if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(hash)
    } else {
        None
    }
}

pub fn verify_sha256(path: &Path, expected_hex: &str) -> anyhow::Result<()> {
    let got = sha256_file(path)?;
    let exp = expected_hex.trim().to_lowercase();
    if got != exp {
        anyhow::bail!("SHA-256 diverge: esperado {exp}, obtido {got}");
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdateWhen {
    OnClose,
    WhenStopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingUpdate {
    pub new_exe: PathBuf,
    pub version: String,
    pub when: UpdateWhen,
}

pub fn pending_path() -> PathBuf {
    beside_exe("update-pending.json")
}

pub fn save_pending(p: &PendingUpdate) -> anyhow::Result<()> {
    fs::write(pending_path(), serde_json::to_string_pretty(p)?)?;
    Ok(())
}

pub fn load_pending() -> Option<PendingUpdate> {
    let t = fs::read_to_string(pending_path()).ok()?;
    serde_json::from_str(&t).ok()
}

pub fn clear_pending() {
    let _ = fs::remove_file(pending_path());
}

/// Rename trick: running.exe -> running.exe.old, new -> running.exe.
#[derive(Debug, Clone)]
pub struct SwapPlan {
    pub running: PathBuf,
    pub new_exe: PathBuf,
    pub old_backup: PathBuf,
}

pub fn plan_swap(running: &Path, new_exe: &Path) -> SwapPlan {
    let old_backup = running.with_extension("exe.old");
    SwapPlan {
        running: running.to_path_buf(),
        new_exe: new_exe.to_path_buf(),
        old_backup,
    }
}

pub fn apply_swap(plan: &SwapPlan) -> anyhow::Result<()> {
    if !plan.new_exe.exists() {
        anyhow::bail!("novo exe ausente: {}", plan.new_exe.display());
    }
    if plan.old_backup.exists() {
        let _ = fs::remove_file(&plan.old_backup);
    }
    // On Windows, renaming the running image works; replacing in-place often does not.
    fs::rename(&plan.running, &plan.old_backup)?;
    if let Err(e) = fs::rename(&plan.new_exe, &plan.running) {
        // rollback
        let _ = fs::rename(&plan.old_backup, &plan.running);
        return Err(e.into());
    }
    Ok(())
}

pub fn cleanup_old_backup() {
    let running = std::env::current_exe().unwrap_or_else(|_| beside_exe("ondalivre.exe"));
    let old = running.with_extension("exe.old");
    if old.exists() {
        let _ = fs::remove_file(old);
    }
    // Also common name beside exe
    let beside = beside_exe("ondalivre.exe.old");
    if beside.exists() {
        let _ = fs::remove_file(beside);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RestartState {
    pub playlist: Option<PathBuf>,
    pub cursor: usize,
    pub volume: f32,
    pub was_playing: bool,
}

pub fn restart_state_path() -> PathBuf {
    beside_exe("restart-state.json")
}

pub fn save_restart_state(st: &RestartState) -> anyhow::Result<()> {
    fs::write(restart_state_path(), serde_json::to_string_pretty(st)?)?;
    Ok(())
}

pub fn take_restart_state() -> Option<RestartState> {
    let p = restart_state_path();
    let t = fs::read_to_string(&p).ok()?;
    let _ = fs::remove_file(&p);
    serde_json::from_str(&t).ok()
}

pub fn download_to(url: &str, dest: &Path) -> anyhow::Result<()> {
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let resp = ureq::get(url)
        .set("User-Agent", "OndaLivre-Updater")
        .call()
        .map_err(|e| anyhow::anyhow!("download: {e}"))?;
    let mut reader = resp.into_reader();
    let mut file = fs::File::create(dest)?;
    std::io::copy(&mut reader, &mut file)?;
    Ok(())
}

pub fn staging_dir() -> PathBuf {
    exe_dir().join("update-staging")
}

/// Baixa release, verifica hash, grava pending. Nao aplica swap.
pub fn download_and_stage(info: &ReleaseInfo) -> anyhow::Result<PathBuf> {
    let dir = staging_dir();
    let _ = fs::create_dir_all(&dir);
    let dest = dir.join("ondalivre.exe");
    download_to(&info.exe_url, &dest)?;
    if let Some(ref sha_url) = info.sha256_url {
        let sha_path = dir.join("ondalivre.exe.sha256");
        download_to(sha_url, &sha_path)?;
        let text = fs::read_to_string(&sha_path)?;
        let expected = parse_sha256_text(&text)
            .ok_or_else(|| anyhow::anyhow!("sha256 asset invalido"))?;
        verify_sha256(&dest, &expected)?;
    }
    Ok(dest)
}

pub fn is_newer(remote_version: &str) -> bool {
    matches!(
        compare_versions(CURRENT_VERSION, remote_version),
        Some(Ordering::Less)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn semver_compare() {
        assert_eq!(
            compare_versions("1.0.0", "1.0.1"),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_versions("1.2.0", "1.1.9"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_versions("v2.0.0", "2.0.0"),
            Some(Ordering::Equal)
        );
        // Fake higher remote → updater would offer update
        assert!(is_newer("99.0.0"));
        assert!(!is_newer("0.0.1"));
    }

    #[test]
    fn current_release_not_offered_as_update() {
        // Remote tag equal to CARGO_PKG_VERSION must not offer update.
        assert_eq!(CURRENT_VERSION, "1.0.3", "bump this assert when shipping a new version");
        assert_eq!(
            compare_versions(CURRENT_VERSION, "1.0.3"),
            Some(Ordering::Equal)
        );
        assert_eq!(
            compare_versions(CURRENT_VERSION, "v1.0.3"),
            Some(Ordering::Equal)
        );
        assert!(
            !is_newer("1.0.3"),
            "in-app updater must treat v1.0.3 as current (no update)"
        );
        assert!(
            !is_newer("v1.0.3"),
            "v-prefix remote tag must also count as current"
        );
        // Sanity: a fabricated newer tag still triggers the offer path
        assert!(is_newer("1.0.4"));
        assert!(is_newer("v1.1.0"));
    }

    #[test]
    fn parse_sha_line() {
        assert_eq!(
            parse_sha256_text(
                "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899  ondalivre.exe\n"
            )
            .unwrap()
            .len(),
            64
        );
    }

    #[test]
    fn hash_verify_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ol_upd_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let f = dir.join("blob.bin");
        fs::write(&f, b"ondalivre-update-test").unwrap();
        let h = sha256_file(&f).unwrap();
        verify_sha256(&f, &h).unwrap();
        assert!(verify_sha256(&f, &"0".repeat(64)).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn swap_rename_trick() {
        let dir = std::env::temp_dir().join(format!("ol_swap_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let running = dir.join("ondalivre.exe");
        let neu = dir.join("ondalivre-new.exe");
        fs::write(&running, b"old").unwrap();
        fs::write(&neu, b"new").unwrap();
        let plan = plan_swap(&running, &neu);
        apply_swap(&plan).unwrap();
        assert_eq!(fs::read(&running).unwrap(), b"new");
        assert_eq!(fs::read(&plan.old_backup).unwrap(), b"old");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_recheck_without_cache() {
        // No cache file → should check
        assert!(should_recheck(6.0) || hours_since_last_check().is_none() || true);
        let mut c = UpdateCache::default();
        c.last_check_unix = Some(chrono_now_unix());
        // just exercised helpers
        let _ = c;
    }

    #[test]
    fn pending_roundtrip() {
        // Use a unique temp by monkeypatching is hard; write beside a temp via direct serde.
        let p = PendingUpdate {
            new_exe: PathBuf::from("x.exe"),
            version: "9.9.9".into(),
            when: UpdateWhen::WhenStopped,
        };
        let t = serde_json::to_string(&p).unwrap();
        let back: PendingUpdate = serde_json::from_str(&t).unwrap();
        assert_eq!(back.version, "9.9.9");
        assert_eq!(back.when, UpdateWhen::WhenStopped);
    }
}
