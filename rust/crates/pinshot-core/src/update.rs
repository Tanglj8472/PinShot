//! Scheduled update checks (GitHub `*-windows-rust` releases).

use chrono::{DateTime, Local, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};

pub const WINDOWS_RELEASE_SUFFIX: &str = "-windows-rust";
pub const DEFAULT_CHECK_TIME: &str = "09:00";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    /// When enabled, check daily after [`check_time`] while PinShot is running.
    pub check_on_startup: bool,
    /// Local time `HH:MM` (24h) for the daily check window.
    pub check_time: String,
    /// Unix seconds UTC when an update check last completed (0 = never).
    pub last_check_unix: i64,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check_on_startup: true,
            check_time: DEFAULT_CHECK_TIME.into(),
            last_check_unix: 0,
        }
    }
}

impl UpdateSettings {
    pub fn normalized(mut self) -> Self {
        self.check_time = normalize_check_time(&self.check_time);
        self
    }
}

pub fn normalize_check_time(raw: &str) -> String {
    if let Some((h, m)) = parse_check_time(raw) {
        return format!("{h:02}:{m:02}");
    }
    DEFAULT_CHECK_TIME.to_string()
}

pub fn parse_check_time(raw: &str) -> Option<(u32, u32)> {
    let s = raw.trim();
    let (h, m) = s.split_once(':')?;
    let h: u32 = h.trim().parse().ok()?;
    let m: u32 = m.trim().parse().ok()?;
    if h < 24 && m < 60 {
        Some((h, m))
    } else {
        None
    }
}

/// Parse `v0.2.3-windows-rust` or `0.2.3` into semver triple.
pub fn parse_release_version(tag: &str) -> Option<(u32, u32, u32)> {
    let mut s = tag.trim();
    if let Some(rest) = s.strip_prefix('v') {
        s = rest;
    }
    if let Some(base) = s.strip_suffix(WINDOWS_RELEASE_SUFFIX) {
        s = base;
    }
    let mut parts = s.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    let patch: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn version_tuple(version: &str) -> Option<(u32, u32, u32)> {
    parse_release_version(version)
}

pub fn is_newer(remote: (u32, u32, u32), local: (u32, u32, u32)) -> bool {
    remote > local
}

pub fn now_unix() -> i64 {
    Local::now().timestamp()
}

/// True when local time is past today's [`check_time`] and we have not checked since then.
pub fn due_for_check(settings: &UpdateSettings, now: DateTime<Local>) -> bool {
    if !settings.check_on_startup {
        return false;
    }
    let (h, m) = parse_check_time(&settings.check_time).unwrap_or((9, 0));
    let Some(naive_time) = NaiveTime::from_hms_opt(h, m, 0) else {
        return false;
    };
    let date = now.date_naive();
    let Some(scheduled_naive) = date.and_time(naive_time).and_local_timezone(Local).single() else {
        return false;
    };
    if now < scheduled_naive {
        return false;
    }
    let scheduled_unix = scheduled_naive.timestamp();
    settings.last_check_unix < scheduled_unix
}

pub fn due_for_check_now(settings: &UpdateSettings) -> bool {
    due_for_check(settings, Local::now())
}

pub fn format_last_check_unix(unix: i64) -> Option<String> {
    if unix <= 0 {
        return None;
    }
    Local
        .timestamp_opt(unix, 0)
        .single()
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn parse_tag_and_compare() {
        assert_eq!(
            parse_release_version("v0.2.3-windows-rust"),
            Some((0, 2, 3))
        );
        assert!(is_newer((0, 2, 4), (0, 2, 3)));
        assert!(!is_newer((0, 2, 3), (0, 2, 3)));
    }

    #[test]
    fn due_after_scheduled_time() {
        let settings = UpdateSettings {
            check_on_startup: true,
            check_time: "09:00".into(),
            last_check_unix: 0,
        };
        let now = Local.with_ymd_and_hms(2026, 10, 14, 10, 0, 0).unwrap();
        assert!(due_for_check(&settings, now));
        let mut done = settings.clone();
        done.last_check_unix = Local.with_ymd_and_hms(2026, 10, 14, 10, 5, 0).unwrap().timestamp();
        assert!(!due_for_check(&done, now));
    }

    #[test]
    fn not_due_before_scheduled_time() {
        let settings = UpdateSettings::default();
        let now = Local.with_ymd_and_hms(2026, 10, 14, 8, 30, 0).unwrap();
        assert!(!due_for_check(&settings, now));
    }
}
