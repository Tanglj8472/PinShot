//! GitHub release check and Windows silent install.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use pinshot_core::config::data_dir;
use pinshot_core::update::{is_newer, parse_release_version, version_tuple, WINDOWS_RELEASE_SUFFIX};
use pinshot_core::VERSION;
use serde::Deserialize;

const REPO: &str = "Tanglj8472/pinshot";
const USER_AGENT: &str = concat!("PinShot/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    UpToDate,
    /// Newer release exists; auto-install is not available on this OS.
    UpdateAvailable { version: String },
    /// Installer launched; the app should exit.
    Installing { version: String },
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

pub fn check_and_apply(auto: bool) -> Result<UpdateOutcome, String> {
    let _ = auto;
    let current = version_tuple(VERSION).ok_or("invalid current version")?;
    let release = fetch_latest_windows_release()?;
    let remote = parse_release_version(&release.version).ok_or("invalid release version")?;
    if !is_newer(remote, current) {
        return Ok(UpdateOutcome::UpToDate);
    }
    #[cfg(windows)]
    {
        let path = download_setup(&release)?;
        launch_installer(&path)?;
        return Ok(UpdateOutcome::Installing {
            version: release.version,
        });
    }
    #[cfg(not(windows))]
    {
        let _ = release;
        Ok(UpdateOutcome::UpdateAvailable {
            version: format!("{}.{}.{}", remote.0, remote.1, remote.2),
        })
    }
}

struct ReleaseInfo {
    version: String,
    setup_url: String,
}

fn fetch_latest_windows_release() -> Result<ReleaseInfo, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=20");
    let resp = ureq::get(&url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", USER_AGENT)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| format!("GitHub releases: {e}"))?;
    let releases: Vec<GhRelease> = resp
        .into_json()
        .map_err(|e| format!("parse releases: {e}"))?;
    for rel in releases {
        if !rel.tag_name.ends_with(WINDOWS_RELEASE_SUFFIX) {
            continue;
        }
        let Some(ver) = parse_release_version(&rel.tag_name) else {
            continue;
        };
        let version = format!("{}.{}.{}", ver.0, ver.1, ver.2);
        let setup = rel
            .assets
            .iter()
            .find(|a| a.name.ends_with("-setup.exe"))
            .ok_or_else(|| format!("no setup.exe on {}", rel.tag_name))?;
        return Ok(ReleaseInfo {
            version,
            setup_url: setup.browser_download_url.clone(),
        });
    }
    Err("no windows-rust release found".into())
}

fn download_setup(release: &ReleaseInfo) -> Result<PathBuf, String> {
    let dir = data_dir().join("updates");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = format!("PinShot-{}-windows-x64-setup.exe", release.version);
    let dest = dir.join(&name);
    if dest.is_file() {
        if dest.metadata().map(|m| m.len()).unwrap_or(0) > 512 * 1024 {
            return Ok(dest);
        }
    }
    crate::logutil::log(&format!("update download {}", release.setup_url));
    let resp = ureq::get(&release.setup_url)
        .set("User-Agent", USER_AGENT)
        .timeout(std::time::Duration::from_secs(600))
        .call()
        .map_err(|e| format!("download setup: {e}"))?;
    let mut reader = resp.into_reader();
    let mut bytes = Vec::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
    }
    if bytes.len() < 512 * 1024 {
        return Err("downloaded installer too small".into());
    }
    let tmp = dest.with_extension("part");
    fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

#[cfg(windows)]
fn launch_installer(path: &Path) -> Result<(), String> {
    crate::logutil::log(&format!("update launch {}", path.display()));
    std::process::Command::new(path)
        .args([
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/CLOSEAPPLICATIONS",
            "/RESTARTAPPLICATIONS",
        ])
        .spawn()
        .map_err(|e| format!("spawn installer: {e}"))?;
    Ok(())
}

#[cfg(not(windows))]
fn launch_installer(_path: &Path) -> Result<(), String> {
    Err("auto-install is Windows-only".into())
}
