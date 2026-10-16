//! File log under the data dir; `--debug` / `PINSHOT_DEBUG` also keep stderr (and a
//! console on Windows release builds that use `windows_subsystem = "windows"`).

use pinshot_core::config::data_dir;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::{OnceLock, mpsc};
use std::time::Instant;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LOG_RETENTION: Duration = Duration::from_secs(2 * 24 * 60 * 60);

static WRITER: OnceLock<mpsc::SyncSender<LogCommand>> = OnceLock::new();
static DROPPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

enum LogCommand {
    Record(String),
    Flush(mpsc::Sender<()>),
}

fn start_writer(mut file: LogFile) -> std::io::Result<mpsc::SyncSender<LogCommand>> {
    // Even a burst of maximum-sized records is bounded to roughly 16 MiB.
    let (tx, rx) = mpsc::sync_channel(256);
    std::thread::Builder::new().name("pinshot-log".into()).spawn(move || {
        while let Ok(command) = rx.recv() {
            match command {
                LogCommand::Record(line) => {
                    let dropped = DROPPED.swap(0, std::sync::atomic::Ordering::Relaxed);
                    if dropped > 0 {
                        let notice = format_record(&timestamp(), 'E', std::process::id(),
                            &format!("logger: queue full; {dropped} records dropped"));
                        if file.write(&notice).is_err() { report_write_failure(); }
                    }
                    if DEBUG.load(std::sync::atomic::Ordering::Relaxed) { eprintln!("{line}"); }
                    if file.write(&line).is_err() {
                        report_write_failure();
                    } else {
                        WRITE_FAILED.store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                LogCommand::Flush(done) => {
                    let dropped = DROPPED.swap(0, std::sync::atomic::Ordering::Relaxed);
                    if dropped > 0 {
                        let notice = format_record(&timestamp(), 'E', std::process::id(),
                            &format!("logger: queue full; {dropped} records dropped"));
                        if file.write(&notice).is_err() { report_write_failure(); }
                    }
                    let _ = done.send(());
                }
            }
        }
    })?;
    Ok(tx)
}

struct LogFile {
    dir: std::path::PathBuf,
    lock: std::fs::File,
}

impl LogFile {
    fn open(dir: &std::path::Path) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_owned(),
            lock: OpenOptions::new().create(true).truncate(false).read(true).write(true)
                .open(dir.join("pinshot.lock"))?,
        })
    }

    fn write(&mut self, line: &str) -> std::io::Result<()> {
        // All processes close the data file before releasing this lock, so
        // rotation never leaves a settings process writing to an old handle.
        fs2::FileExt::lock_exclusive(&self.lock)?;
        let result = self.write_locked(line);
        let unlocked = fs2::FileExt::unlock(&self.lock);
        result.and(unlocked)
    }

    fn write_locked(&self, line: &str) -> std::io::Result<()> {
        const LIMIT: u64 = 10 * 1024 * 1024;
        let date = line.get(1..11)
            .filter(|date| date.as_bytes().get(4) == Some(&b'-') && date.as_bytes().get(7) == Some(&b'-'))
            .unwrap_or("test");
        let path = self.dir.join(format!("pinshot-{date}.txt"));
        if fs::metadata(&path).map(|m| m.len() + line.len() as u64 + 1 > LIMIT).unwrap_or(false) {
            let backup = self.dir.join(format!("pinshot-{date}.txt.1"));
            match fs::remove_file(&backup) {
                Ok(()) => {},
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
                Err(e) => return Err(e),
            }
            fs::rename(&path, backup)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{line}")?;
        file.flush()
    }
}

/// Keep diagnostic logs for at most two days.  The active log is never
/// removed; old size-rotation files (for example `pinshot.log.1`) are
/// removed on the next startup once their last-write time is older than the
/// retention window.
fn prune_old_logs(dir: &std::path::Path) {
    let Ok(entries) = fs::read_dir(dir) else { return; };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue; };
        if !name.starts_with("pinshot-") || !name.ends_with(".txt.1") {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else { continue; };
        let expired = now.duration_since(modified).map(|age| age > LOG_RETENTION).unwrap_or(false);
        if expired {
            let _ = fs::remove_file(path);
        }
    }
}
static DEBUG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static LEVEL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);
static WRITE_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn report_write_failure() {
    if !WRITE_FAILED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        // Do not recurse into the failing file logger or echo paths/secrets.
        eprintln!("PinShot: diagnostic log could not be written; check disk space and permissions.");
    }
}

fn level_value(level: &str) -> u8 {
    match level { "off" => 0, "trace" | "detailed" => 2, "debug" => 3, _ => 1 }
}

fn enabled(configured: u8, level: &str) -> bool {
    configured != 0 && level != "off" && level_value(level) <= configured
}

fn timestamp() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let millis = now.subsec_millis();
    let days = (now.as_secs() / 86_400) as i64;
    let sec = now.as_secs() % 86_400;
    // Civil date conversion from Unix days, emitted in UTC for portability.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    format!("{year:04}-{m:02}-{d:02} {:02}:{:02}:{:02}.{:03}", sec / 3600, (sec / 60) % 60, sec % 60, millis)
}

pub fn set_level(level: &str) {
    let effective = if DEBUG.load(std::sync::atomic::Ordering::Relaxed) { 3 } else { level_value(level) };
    LEVEL.store(effective, std::sync::atomic::Ordering::Relaxed);
}

pub fn debug_enabled() -> bool {
    LEVEL.load(std::sync::atomic::Ordering::Relaxed) >= 3
}

pub fn init(args: &[String]) {
    let debug = args.iter().any(|a| a == "--debug")
        || std::env::var("PINSHOT_DEBUG")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false);
    DEBUG.store(debug, std::sync::atomic::Ordering::Relaxed);
    let cfg = pinshot_core::config::load().unwrap_or_default();
    set_level(&cfg.log_level);
    if debug {
        attach_console();
    }
    let dir = data_dir();
    prune_old_logs(&dir);
    match LogFile::open(&dir).and_then(start_writer) {
        Ok(writer) => { let _ = WRITER.set(writer); },
        Err(_) => report_write_failure(),
    }
    log(&format!(
        "PinShot {} starting debug={debug}",
        pinshot_core::VERSION
    ));
    mem("startup");
    install_panic_hook();
}

fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        error(&format!("PANIC: {info}\n{bt}"));
        flush();
        prev(info);
    }));
}

#[track_caller]
pub fn ms(label: &str, started: Instant) {
    log_at("trace", &format!("{label} {}ms", started.elapsed().as_millis()));
}

/// Optional diagnostic pause between screenshot stages. Disabled unless the
/// caller sets PINSHOT_STEP_DELAY_MS to a positive integer. The pause is
/// deliberately explicit and trace-logged so local performance experiments
/// can identify the stage that owns the visible delay.
pub fn step(label: &str) {
    let Some(ms) = std::env::var("PINSHOT_STEP_DELAY_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&v| v > 0)
    else {
        return;
    };
    log_at("trace", &format!("step {label}: pause {ms}ms"));
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

/// Process memory snapshot for `--debug` tracing.
///
/// `PrivateMemorySize64` is deliberately *not* the headline number: on a machine
/// with virtual display adapters and a multi-vendor GPU stack, a D3D device makes
/// the driver reserve large committed-but-untouched ranges, and Private then reads
/// in the gigabytes while the process itself holds almost nothing. `WorkingSet` is
/// the only figure that reflects real physical residency, so it goes first.
#[derive(Clone, Copy, Default)]
pub struct MemSnapshot {
    pub working_set_mb: f64,
    pub private_mb: f64,
    pub peak_working_set_mb: f64,
    pub handles: u32,
}

impl MemSnapshot {
    pub fn current() -> Self {
        #[cfg(windows)]
        {
            use windows::Win32::System::ProcessStatus::{
                GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
            };
            use windows::Win32::System::Threading::GetCurrentProcess;
            let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
            // SAFETY: `c` is a live, correctly sized out-param, and the pseudo
            // handle from GetCurrentProcess is always valid for this thread.
            let ok = unsafe {
                GetProcessMemoryInfo(
                    GetCurrentProcess(),
                    &mut c as *mut _ as *mut _,
                    std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                )
            }
            .is_ok();
            if !ok {
                return Self::default();
            }
            const MB: f64 = 1024.0 * 1024.0;
            Self {
                working_set_mb: c.WorkingSetSize as f64 / MB,
                private_mb: c.PrivateUsage as f64 / MB,
                peak_working_set_mb: c.PeakWorkingSetSize as f64 / MB,
                handles: 0,
            }
        }
        #[cfg(not(windows))]
        {
            Self::default()
        }
    }

    /// One-line form for `--debug` logs. WorkingSet leads because it is the
    /// figure that reflects actual physical use.
    pub fn line(&self, label: &str) -> String {
        format!(
            "mem {label} ws={:.1}MB peak_ws={:.1}MB priv={:.1}MB",
            self.working_set_mb, self.peak_working_set_mb, self.private_mb
        )
    }
}

/// Log a memory snapshot when `--debug` is on. Silent otherwise so release
/// builds pay nothing beyond one relaxed atomic load.
#[track_caller]
pub fn mem(label: &str) {
    if LEVEL.load(std::sync::atomic::Ordering::Relaxed) >= 3 {
        log_at("debug", &MemSnapshot::current().line(label));
    }
}

#[track_caller]
pub fn log(msg: &str) {
    log_at("normal", msg);
}

/// Errors are included in every enabled logging mode.
#[track_caller]
pub fn error(msg: &str) {
    log_at("error", msg);
}

fn format_record(stamp: &str, tag: char, pid: u32, msg: &str) -> String {
    const MAX_RECORD_BYTES: usize = 64 * 1024;
    let label = match tag {
        'T' => "追踪".to_owned(),
        other => other.to_string(),
    };
    let prefix = format!("[{stamp} UTC] [{label}] [pid={pid}] ");
    let mut record = String::new();
    for (i, line) in msg.split('\n').enumerate() {
        if record.len() + prefix.len() + 32 >= MAX_RECORD_BYTES {
            record.push_str(" [record truncated]");
            return record;
        }
        if i != 0 { record.push('\n'); }
        record.push_str(&prefix);
        for ch in line.chars() {
            if record.len() + 32 >= MAX_RECORD_BYTES {
                record.push_str(" [record truncated]");
                return record;
            }
            // Prevent embedded terminal controls from changing how diagnostics
            // are displayed; retain useful tabs in backtraces.
            if ch.is_control() && ch != '\t' {
                record.extend(ch.escape_default());
            } else {
                record.push(ch);
            }
        }
    }
    record
}

#[track_caller]
pub fn log_at(level: &str, msg: &str) {
    let configured = LEVEL.load(std::sync::atomic::Ordering::Relaxed);
    if !enabled(configured, level) {
        return;
    }
    let tag = if level == "error" { 'E' } else { match level_value(level) {
        3 => 'D',
        2 => 'T',
        1 => 'I',
        _ => 'O',
    }};
    let caller = std::panic::Location::caller();
    let module = caller.file().rsplit(['/', '\\']).next().unwrap_or("unknown");
    let line = format_record(&timestamp(), tag, std::process::id(),
        &format!("[{module}:{}] {msg}", caller.line()));
    if let Some(writer) = WRITER.get() {
        match writer.try_send(LogCommand::Record(line)) {
            Ok(()) => {},
            Err(mpsc::TrySendError::Full(_)) => {
                DROPPED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Err(mpsc::TrySendError::Disconnected(_)) => report_write_failure(),
        }
    }
}

pub fn flush() {
    let Some(writer) = WRITER.get() else { return; };
    let (tx, rx) = mpsc::channel();
    let mut command = LogCommand::Flush(tx);
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match writer.try_send(command) {
            Ok(()) => { let _ = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())); break; }
            Err(mpsc::TrySendError::Full(returned)) if Instant::now() < deadline => {
                command = returned;
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            _ => break,
        }
    }
}

fn attach_console() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Console::{AllocConsole, AttachConsole, ATTACH_PARENT_PROCESS};
        if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            let _ = AllocConsole();
        }
        crate::win32::ignore_ctrl_c();
    }
}

/// Release the parent PowerShell/cmd so typing works after we `process::exit`.
pub fn detach_console() {
    flush();
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Console::FreeConsole;
        let _ = FreeConsole();
    }
}

/// `--version` / `--help` must be visible even when the exe is a Windows GUI subsystem binary.
pub fn ensure_stdio() {
    attach_console();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_records_use_chinese_label() {
        let record = format_record("2026-10-15 12:00:00.123", 'T', 42, "first\nsecond");
        assert_eq!(record.lines().count(), 2);
        assert!(record.lines().all(|line| line.contains("[追踪] [pid=42]")));
        assert!(!record.contains("[T]"));
    }

    #[test]
    fn background_flush_acknowledges_written_records() {
        let dir = std::env::temp_dir().join(format!("pinshot-log-worker-{}-{}",
            std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let writer = start_writer(LogFile::open(&dir).unwrap()).unwrap();
        for i in 0..100 {
            writer.send(LogCommand::Record(format!("event {i}"))).unwrap();
        }
        let (tx, rx) = mpsc::channel();
        writer.send(LogCommand::Flush(tx)).unwrap();
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let content = fs::read_to_string(dir.join("pinshot-test.txt")).unwrap();
        assert_eq!(content.lines().count(), 100);
        assert!(content.ends_with("event 99\n"));
        drop(writer);
        fs::remove_file(dir.join("pinshot-test.txt")).unwrap();
        fs::remove_file(dir.join("pinshot.lock")).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn independent_writers_preserve_complete_records() {
        let dir = std::env::temp_dir().join(format!("pinshot-log-concurrent-{}-{}",
            std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let started = Instant::now();
        let workers: Vec<_> = (0..4).map(|worker| {
            let dir = dir.clone();
            std::thread::spawn(move || {
                let mut writer = LogFile::open(&dir).unwrap();
                for record in 0..250 {
                    writer.write(&format!("worker={worker} record={record}")).unwrap();
                }
            })
        }).collect();
        for worker in workers { worker.join().unwrap(); }
        let content = fs::read_to_string(dir.join("pinshot-test.txt")).unwrap();
        let actual: std::collections::HashSet<_> = content.lines().collect();
        assert_eq!(content.lines().count(), 1000);
        for worker in 0..4 {
            for record in 0..250 {
                assert!(actual.contains(format!("worker={worker} record={record}").as_str()));
            }
        }
        eprintln!("1000 records, four independent writers: {:?}", started.elapsed());
        fs::remove_file(dir.join("pinshot-test.txt")).unwrap();
        fs::remove_file(dir.join("pinshot.lock")).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn oversized_records_are_bounded_at_unicode_boundaries() {
        for message in ["日志".repeat(100_000), "\n".repeat(100_000)] {
            let record = format_record("2026-10-15 12:00:00.123", 'E', 1, &message);
            assert!(record.len() <= 64 * 1024);
            assert!(record.ends_with("[record truncated]"));
        }
    }

    #[test]
    fn multiline_records_keep_context_and_escape_controls() {
        let record = format_record("2026-10-15 12:00:00.123", 'E', 42, "failed\nframe\r\x1b[2J");
        let lines: Vec<_> = record.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| line.starts_with("[2026-10-15 12:00:00.123 UTC] [E] [pid=42] ")));
        assert!(!record.contains('\r'));
        assert!(!record.contains('\x1b'));
    }

    #[test]
    fn configured_levels_filter_records() {
        for level in ["normal", "error", "trace", "debug", "off"] {
            assert!(!enabled(0, level));
        }
        assert!(enabled(1, "normal"));
        assert!(enabled(1, "error"));
        assert!(!enabled(1, "trace"));
        assert!(!enabled(1, "debug"));
        assert!(enabled(2, "trace"));
        assert!(!enabled(2, "debug"));
        assert!(enabled(3, "trace"));
        assert!(enabled(3, "debug"));
        assert!(!enabled(3, "off"));
        assert_eq!(level_value("detailed"), level_value("trace"));
    }

    #[test]
    fn rotation_reopens_file_for_other_writers() {
        let dir = std::env::temp_dir().join(format!("pinshot-log-test-{}-{}",
            std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let mut first = LogFile::open(&dir).unwrap();
        let mut second = LogFile::open(&dir).unwrap();
        first.write("previous session").unwrap();
        OpenOptions::new().write(true).open(dir.join("pinshot-test.txt")).unwrap()
            .set_len(10 * 1024 * 1024).unwrap();
        second.write("after rotation").unwrap();
        first.write("original writer continued").unwrap();
        assert_eq!(fs::read_to_string(dir.join("pinshot-test.txt")).unwrap(),
            "after rotation\noriginal writer continued\n");
        assert_eq!(fs::metadata(dir.join("pinshot-test.txt.1")).unwrap().len(), 10 * 1024 * 1024);
        drop(first);
        drop(second);
        for name in ["pinshot-test.txt", "pinshot-test.txt.1", "pinshot.lock"] {
            fs::remove_file(dir.join(name)).unwrap();
        }
        fs::remove_dir(dir).unwrap();
    }
}
