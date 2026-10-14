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
const PROCESS_NAME: &str = "pinshot.exe";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    UpToDate,
    /// Newer release exists; auto-install is not available on this OS.
    UpdateAvailable { version: String },
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
pub fn defer_install_after_quit(setup: PathBuf) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["--install-after-quit", &setup.to_string_lossy()]);
    use std::os::windows::process::CommandExt;
    use windows::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};
    let _ = cmd
        .creation_flags(CREATE_NEW_PROCESS_GROUP.0 | DETACHED_PROCESS.0)
        .spawn();
}

#[cfg(not(windows))]
pub fn defer_install_after_quit(_setup: PathBuf) {}

/// Inno Setup / manual upgrade: ask the running app to quit and wait for file unlock.
#[cfg(windows)]
pub fn quit_for_install() {
    crate::logutil::ensure_stdio();
    let _ = crate::ipc::send("quit");
    wait_for_other_pinshot_exit(20);
    terminate_other_pinshot();
    std::thread::sleep(std::time::Duration::from_millis(400));
}

#[cfg(not(windows))]
pub fn quit_for_install() {}

#[cfg(windows)]
pub fn run_install_after_quit(setup: &Path) -> Result<(), String> {
    crate::logutil::ensure_stdio();
    let _ = crate::ipc::send("quit");
    wait_for_other_pinshot_exit(45);
    terminate_other_pinshot();
    std::thread::sleep(std::time::Duration::from_millis(400));
    if !pinshot_pids(current_pid()).is_empty() {
        return Err("PinShot is still running; close it and retry".into());
    }
    launch_installer(setup)?;
    std::process::exit(0);
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
            "/CLOSEAPPLICATIONS",
            "/RESTARTAPPLICATIONS",
        ])
        .spawn()
        .map_err(|e| format!("spawn installer: {e}"))?;
    Ok(())
}

#[cfg(windows)]
fn current_pid() -> u32 {
    unsafe { windows::Win32::System::Threading::GetCurrentProcessId() }
}

#[cfg(windows)]
fn pinshot_pids(exclude: u32) -> Vec<u32> {
    use std::mem::size_of;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };

    let mut out = Vec::new();
    unsafe {
        let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return out,
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
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

#[cfg(windows)]
fn wait_for_other_pinshot_exit(max_secs: u32) {
    let me = current_pid();
    for i in 0..max_secs.saturating_mul(2) {
        if pinshot_pids(me).is_empty() {
            return;
        }
        if i == max_secs.saturating_mul(2) / 2 {
            terminate_other_pinshot();
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

#[cfg(windows)]
fn terminate_other_pinshot() {
    let me = current_pid();
    for pid in pinshot_pids(me) {
        terminate_pid(pid);
    }
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
