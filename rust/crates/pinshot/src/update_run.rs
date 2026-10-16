//! GitHub release check and Windows silent install.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use pinshot_core::config::data_dir;
use pinshot_core::update::{
    is_newer, parse_release_version, version_tuple, WINDOWS_RELEASE_SUFFIX,
};
use pinshot_core::VERSION;
use serde::Deserialize;

const REPO: &str = "Tanglj8472/pinshot";
const USER_AGENT: &str = concat!("PinShot/", env!("CARGO_PKG_VERSION"));
const PROCESS_NAME: &str = "pinshot.exe";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    UpToDate,
    /// Newer release exists; auto-install is not available on this OS.
    UpdateAvailable {
        version: String,
    },
    /// Downloaded installer; caller must quit all instances then run it.
    ReadyToInstall {
        version: String,
        setup_path: PathBuf,
    },
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
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
        return Ok(UpdateOutcome::ReadyToInstall {
            version: release.version,
            setup_path: path,
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

/// Tray/settings: spawn a helper that waits for PinShot to exit, then runs setup.
#[cfg(windows)]
pub fn defer_install_after_quit(setup: PathBuf) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["--install-after-quit", &setup.to_string_lossy()]);
    use std::os::windows::process::CommandExt;
    use windows::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};
    cmd.creation_flags(CREATE_NEW_PROCESS_GROUP.0 | DETACHED_PROCESS.0)
        .spawn()
        .map_err(|e| format!("start update helper: {e}"))?;
    Ok(())
}

#[cfg(not(windows))]
pub fn defer_install_after_quit(_setup: PathBuf) -> Result<(), String> {
    Err("Automatic installation is only supported on Windows".into())
}

/// Inno Setup / manual upgrade: ask the running app to quit and wait for file unlock.
#[cfg(windows)]
pub fn quit_for_install(install_dir: Option<&Path>) -> Result<(), String> {
    crate::logutil::ensure_stdio();
    let all = pinshot_pids(current_pid())?;
    let targets = scoped_pinshot_pids(install_dir)?;
    // The pipe is global to the user. Do not ask an unrelated installation to
    // quit when upgrading a different directory (including isolated tests).
    if !targets.is_empty() && targets == all {
        // A stalled IPC server must not prevent the timed process cleanup.
        request_quit_for_install();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let force_at = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let targets = scoped_pinshot_pids(install_dir)?;
        if targets.is_empty() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(
                "PinShot is still running; close it or check its permissions and retry".into(),
            );
        }
        if std::time::Instant::now() >= force_at {
            for pid in targets {
                terminate_pid(pid);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(not(windows))]
pub fn quit_for_install(_install_dir: Option<&Path>) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn scoped_pinshot_pids(install_dir: Option<&Path>) -> Result<Vec<u32>, String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let pids = pinshot_pids(current_pid())?;
    let Some(dir) = install_dir else {
        return Ok(pids);
    };
    let expected = match fs::canonicalize(dir) {
        Ok(path) => path,
        // A fresh installation has no destination directory and no files to unlock.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("installer directory: {e}")),
    };
    let expected = expected.to_string_lossy().to_lowercase();
    let mut out = Vec::new();
    for pid in pids {
        unsafe {
            let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                Ok(h) => h,
                // A process may have exited since the snapshot was taken.
                Err(_) if !pinshot_pids(current_pid())?.contains(&pid) => continue,
                Err(e) => return Err(format!("cannot inspect PinShot process {pid}: {e}")),
            };
            let mut buffer = vec![0u16; 32768];
            let mut len = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buffer.as_mut_ptr()),
                &mut len,
            );
            let mut exit_code = 259u32; // STILL_ACTIVE
            let exited = windows::Win32::System::Threading::GetExitCodeProcess(handle, &mut exit_code)
                .is_ok() && exit_code != 259;
            let _ = CloseHandle(handle);
            if let Err(e) = result {
                // The process can exit after the Toolhelp snapshot while
                // QueryFullProcessImageNameW is still inspecting it. The
                // opened handle, unlike another snapshot, reliably reports
                // whether that exact process has terminated.
                if exited {
                    continue;
                }
                if !pinshot_pids(current_pid())?.contains(&pid) {
                    continue;
                }
                return Err(format!("PinShot process path {pid}: {e}"));
            }
            let exe = PathBuf::from(String::from_utf16_lossy(&buffer[..len as usize]));
            if let Some(parent) = exe.parent() {
                let parent = fs::canonicalize(parent)
                    .map_err(|e| format!("PinShot process directory: {e}"))?;
                if parent.to_string_lossy().to_lowercase() == expected {
                    out.push(pid);
                }
            }
        }
    }
    Ok(out)
}

#[cfg(windows)]
fn request_quit_for_install() {
    std::thread::spawn(|| {
        if let Err(e) = crate::ipc::send("quit") {
            crate::logutil::log(&format!("installer quit IPC: {e}"));
        }
    });
}

#[cfg(windows)]
pub fn run_install_after_quit(setup: &Path) -> Result<(), String> {
    crate::logutil::ensure_stdio();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let install_dir = exe.parent().ok_or("update helper has no directory")?;
    quit_for_install(Some(install_dir))?;
    launch_installer(setup, install_dir)?;
    crate::logutil::flush();
    std::process::exit(0);
}

struct ReleaseInfo {
    version: String,
    setup_url: String,
    setup_name: String,
    digest: Option<String>,
    checksum_url: Option<String>,
}

fn fetch_latest_windows_release() -> Result<ReleaseInfo, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=100");
    let resp = ureq::get(&url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", USER_AGENT)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| format!("GitHub releases: {e}"))?;
    let body = resp
        .into_string()
        .map_err(|e| format!("read releases: {e}"))?;
    let releases: Vec<GhRelease> =
        serde_json::from_str(&body).map_err(|e| format!("parse releases: {e}"))?;
    select_release(&releases)
}

fn select_release(releases: &[GhRelease]) -> Result<ReleaseInfo, String> {
    let (ver, rel, setup) = releases
        .iter()
        .filter_map(|rel| {
            if rel.draft || rel.prerelease || !rel.tag_name.ends_with(WINDOWS_RELEASE_SUFFIX) {
                return None;
            }
            let ver = parse_release_version(&rel.tag_name)?;
            let name = format!(
                "PinShot-{}.{}.{}-windows-x64-setup.exe",
                ver.0, ver.1, ver.2
            );
            let setup = rel.assets.iter().find(|asset| asset.name == name)?;
            Some((ver, rel, setup))
        })
        .max_by_key(|(ver, _, _)| *ver)
        .ok_or("no stable Windows installer release found")?;
    Ok(ReleaseInfo {
        version: format!("{}.{}.{}", ver.0, ver.1, ver.2),
        setup_url: setup.browser_download_url.clone(),
        setup_name: setup.name.clone(),
        digest: setup.digest.clone(),
        checksum_url: rel
            .assets
            .iter()
            .find(|a| a.name == "SHA256SUMS.txt")
            .map(|a| a.browser_download_url.clone()),
    })
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}

fn checksum_from_manifest(text: &str, name: &str) -> Result<String, String> {
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        if let (Some(hash), Some(file)) = (fields.next(), fields.next()) {
            if file.trim_start_matches('*') == name && valid_sha256(hash) {
                return Ok(hash.to_ascii_lowercase());
            }
        }
    }
    Err("installer SHA-256 missing from checksum manifest".into())
}

fn expected_setup_digest(release: &ReleaseInfo) -> Result<String, String> {
    if let Some(hash) = release
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
    {
        if valid_sha256(hash) {
            return Ok(hash.to_ascii_lowercase());
        }
    }
    let url = release
        .checksum_url
        .as_ref()
        .ok_or("release has no installer SHA-256")?;
    let text = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| format!("fetch checksums: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    checksum_from_manifest(&text, &release.setup_name)
}

fn sha256(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn verify_setup(bytes: &[u8], expected: &str) -> Result<(), String> {
    if bytes.len() < 512 * 1024 || !bytes.starts_with(b"MZ") {
        return Err("download is not a valid-size Windows installer".into());
    }
    if sha256(bytes) != expected {
        return Err("installer SHA-256 mismatch".into());
    }
    Ok(())
}

fn download_setup(release: &ReleaseInfo) -> Result<PathBuf, String> {
    let dir = data_dir().join("updates");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let expected = expected_setup_digest(release)?;
    let dest = dir.join(&release.setup_name);
    if let Ok(bytes) = fs::read(&dest) {
        if verify_setup(&bytes, &expected).is_ok() {
            return Ok(dest);
        }
        crate::logutil::log("update: invalid cached installer, downloading again");
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
    verify_setup(&bytes, &expected)?;
    let tmp = dest.with_extension("part");
    fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

#[cfg(windows)]
fn launch_installer(path: &Path, install_dir: &Path) -> Result<(), String> {
    crate::logutil::log(&format!("update launch {}", path.display()));
    fs::create_dir_all(data_dir().join("updates")).map_err(|e| e.to_string())?;
    std::process::Command::new(path)
        .args([
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/CLOSEAPPLICATIONS",
            "/NORESTART",
            "/PINSHOTRESTART=1",
        ])
        .arg(format!("/DIR={}", install_dir.display()))
        .arg(format!(
            "/LOG={}",
            data_dir().join("updates").join("install.log").display()
        ))
        .spawn()
        .map_err(|e| format!("spawn installer: {e}"))?;
    Ok(())
}

#[cfg(windows)]
fn current_pid() -> u32 {
    unsafe { windows::Win32::System::Threading::GetCurrentProcessId() }
}

#[cfg(windows)]
fn pinshot_pids(exclude: u32) -> Result<Vec<u32>, String> {
    use std::mem::size_of;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_NO_MORE_FILES};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let mut out = Vec::new();
    unsafe {
        let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(e) => return Err(format!("cannot enumerate PinShot processes: {e}")),
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let name = String::from_utf16_lossy(
                    &entry
                        .szExeFile
                        .iter()
                        .take_while(|&&c| c != 0)
                        .copied()
                        .collect::<Vec<_>>(),
                );
                if name.eq_ignore_ascii_case(PROCESS_NAME) && entry.th32ProcessID != exclude {
                    out.push(entry.th32ProcessID);
                }
                if Process32NextW(snap, &mut entry).is_err() {
                    let error = GetLastError();
                    if error != ERROR_NO_MORE_FILES {
                        let _ = CloseHandle(snap);
                        return Err(format!("process enumeration failed: {}", error.0));
                    }
                    break;
                }
            }
        } else {
            let error = GetLastError();
            let _ = CloseHandle(snap);
            return Err(format!("cannot read process snapshot: {}", error.0));
        }
        let _ = CloseHandle(snap);
    }
    Ok(out)
}

#[cfg(windows)]
fn terminate_pid(pid: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) else {
            return;
        };
        let _ = TerminateProcess(handle, 1);
        let _ = CloseHandle(handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_highest_stable_version_not_github_order() {
        let releases: Vec<GhRelease> = serde_json::from_str(r#"[
            {"tag_name":"v0.2.1-windows-rust","assets":[{"name":"PinShot-0.2.1-windows-x64-setup.exe","browser_download_url":"old"}]},
            {"tag_name":"v9.0.0-windows-rust","prerelease":true,"assets":[{"name":"PinShot-9.0.0-windows-x64-setup.exe","browser_download_url":"preview"}]},
            {"tag_name":"v8.0.0-windows-rust","draft":true,"assets":[{"name":"PinShot-8.0.0-windows-x64-setup.exe","browser_download_url":"draft"}]},
            {"tag_name":"v0.2.7-windows-rust","assets":[]},
            {"tag_name":"v0.2.6-windows-rust","assets":[{"name":"PinShot-0.2.6-windows-x64-setup.exe","browser_download_url":"new"}]}
        ]"#).unwrap();
        let selected = select_release(&releases).unwrap();
        assert_eq!(selected.version, "0.2.6");
        assert_eq!(selected.setup_url, "new");
    }

    #[test]
    fn rejects_corrupt_cached_installer_and_non_pe_payload() {
        let mut bytes = vec![0u8; 512 * 1024];
        bytes[..2].copy_from_slice(b"MZ");
        let expected = sha256(&bytes);
        assert!(verify_setup(&bytes, &expected).is_ok());
        bytes[1024] = 1;
        assert!(verify_setup(&bytes, &expected).is_err());
        let text = vec![b'x'; 512 * 1024];
        assert!(verify_setup(&text, &sha256(&text)).is_err());
    }

    #[test]
    fn checksum_manifest_requires_exact_file_name_and_sha256() {
        let hash = "a".repeat(64);
        let manifest = format!("{}  other.exe\n{}  *setup.exe\n", "b".repeat(64), hash);
        assert_eq!(
            checksum_from_manifest(&manifest, "setup.exe").unwrap(),
            hash
        );
        assert!(checksum_from_manifest(&manifest, "up.exe").is_err());
        assert!(checksum_from_manifest("bad setup.exe", "setup.exe").is_err());
    }
}

#[cfg(not(windows))]
pub fn run_install_after_quit(_setup: &Path) -> Result<(), String> {
    Err("Automatic installation is only supported on Windows".into())
}
