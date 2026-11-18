use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::Deserialize;

pub const REPO: &str = "LuaXII/PrismDisk";
pub const WEBSITE: &str = "https://prismdisk.vercel.app/";
pub const CHANGELOG: &str = include_str!("../../CHANGELOG.md");

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub asset_url: String,
    pub asset_name: String,
    pub size: u64,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<GhAsset>,
}

pub fn current_version() -> String {
    std::env::var("PRISMDISK_PRETEND_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

fn parse(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches(['v', 'V']).split(['.', '-', '+']).take(3).map(|p| p.parse().unwrap_or(0)).collect()
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (mut a, mut b) = (parse(candidate), parse(current));
    a.resize(3, 0);
    b.resize(3, 0);
    a > b
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(8))
        .timeout_read(std::time::Duration::from_secs(30))
        .user_agent(&format!("PrismDisk/{}", env!("CARGO_PKG_VERSION")))
        .build()
}

pub fn check() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = agent().get(&url).set("Accept", "application/vnd.github+json").call();
    let rel: GhRelease = match resp {
        Ok(r) => r.into_json().map_err(|e| format!("Unexpected reply from GitHub: {e}"))?,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(ureq::Error::Status(code, _)) => return Err(format!("GitHub returned HTTP {code}")),
        Err(e) => return Err(format!("Could not reach GitHub: {e}")),
    };
    if rel.draft || rel.prerelease || !is_newer(&rel.tag_name, &current_version()) {
        return Ok(None);
    }
    let asset = rel
        .assets
        .into_iter()
        .find(|a| a.name.to_lowercase().starts_with("prismdisk-setup") && a.name.to_lowercase().ends_with(".exe"))
        .ok_or("The latest release has no installer attached")?;
    Ok(Some(Release {
        version: rel.tag_name.trim_start_matches(['v', 'V']).to_string(),
        notes: rel.body.unwrap_or_default(),
        asset_url: asset.browser_download_url,
        asset_name: asset.name,
        size: asset.size,
    }))
}

pub fn download(rel: &Release, progress: Arc<AtomicU64>) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("PrismDisk-Update");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(&rel.asset_name);
    if std::fs::metadata(&dest).map(|m| m.len() == rel.size && rel.size > 0).unwrap_or(false) {
        progress.store(rel.size, Ordering::Relaxed);
        return Ok(dest);
    }
    let resp = agent().get(&rel.asset_url).call().map_err(|e| format!("Download failed: {e}"))?;
    let tmp = dir.join(format!("{}.part", rel.asset_name));
    let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        progress.store(done, Ordering::Relaxed);
    }
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    if rel.size > 0 && done != rel.size {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("Download was incomplete ({done} of {} bytes)", rel.size));
    }
    let _ = std::fs::remove_file(&dest);
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

pub fn install(setup: &std::path::Path) -> Result<(), String> {
    if crate::sys::shell_open(&setup.display().to_string(), "--silent --update --launch") {
        Ok(())
    } else {
        Err("The update was cancelled".into())
    }
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn version_compare() {
        assert!(is_newer("v1.0.1", "1.0.0"));
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(!is_newer("v1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.9", "1.0.0"));
    }
}
