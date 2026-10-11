//! Parse human hotkey strings into Win32 `(mods, vk)` pairs.

/// Win32 modifier bits (same values as `MOD_*` in `winuser.h`).
pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;
pub const MOD_NOREPEAT: u32 = 0x4000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub mods: u32,
    pub vk: u32,
}

/// `'Ctrl+Shift+F3'` → `(mods, vk)` for `RegisterHotKey`. `None` if empty/unknown.
pub fn parse_hotkey(seq: &str) -> Option<Hotkey> {
    let seq = seq.trim();
    if seq.is_empty() {
        return None;
    }
    let parts: Vec<&str> = seq
        .split('+')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }
    let mut mods = 0u32;
    for p in &parts[..parts.len() - 1] {
        mods |= match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => MOD_CONTROL,
            "shift" => MOD_SHIFT,
            "alt" => MOD_ALT,
            "win" | "super" | "meta" => MOD_WIN,
            _ => return None,
        };
    }
    let k = parts[parts.len() - 1].to_ascii_lowercase();
    let vk = if k.starts_with('f') && k.len() >= 2 {
        let n: u32 = k[1..].parse().ok()?;
        if (1..=24).contains(&n) {
            0x70 + n - 1
        } else {
            return None;
        }
    } else if k.len() == 1 {
        let c = k.chars().next()?;
        if c.is_ascii_alphabetic() || c.is_ascii_digit() {
            c.to_ascii_uppercase() as u32
        } else {
            named_vk(&k)?
        }
    } else {
        named_vk(&k)?
    };
    Some(Hotkey { mods, vk })
}

fn named_vk(k: &str) -> Option<u32> {
    Some(match k {
        "esc" | "escape" => 0x1B,
        "tab" => 0x09,
        "space" => 0x20,
        "enter" | "return" => 0x0D,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "insert" | "ins" => 0x2D,
        "home" => 0x24,
        "end" => 0x23,
        "pgup" | "pageup" | "page_up" => 0x21,
        "pgdn" | "pagedown" | "page_down" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "print" | "printscreen" | "prtsc" => 0x2C,
        "pause" => 0x13,
        "`" => 0xC0,
        "-" => 0xBD,
        "=" => 0xBB,
        "[" => 0xDB,
        "]" => 0xDD,
        ";" => 0xBA,
        "'" => 0xDE,
        "," => 0xBC,
        "." => 0xBE,
        "/" => 0xBF,
        "\\" => 0xDC,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Snip,
    PinClipboard,
    HideAllPins,
    RestoreClosed,
    ClickThrough,
    Quit,
    OpenSettings,
}

impl Command {
    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "snip" => Self::Snip,
            "pin" => Self::PinClipboard,
            "hide_all" => Self::HideAllPins,
            "restore" => Self::RestoreClosed,
            "clickthrough" => Self::ClickThrough,
            "quit" => Self::Quit,
            "prefs" | "settings" => Self::OpenSettings,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_f_keys() {
        let hk = parse_hotkey("F1").unwrap();
        assert_eq!(hk.mods, 0);
        assert_eq!(hk.vk, 0x70);
        let hk = parse_hotkey("F3").unwrap();
        assert_eq!(hk.vk, 0x72);
        let hk = parse_hotkey("Shift+F3").unwrap();
        assert_eq!(hk.mods, MOD_SHIFT);
        assert_eq!(hk.vk, 0x72);
    }

    #[test]
    fn parse_modifiers() {
        let hk = parse_hotkey("Ctrl+Shift+Z").unwrap();
        assert_eq!(hk.mods, MOD_CONTROL | MOD_SHIFT);
        assert_eq!(hk.vk, b'Z' as u32);
        let hk = parse_hotkey("Ctrl+Alt+T").unwrap();
        assert_eq!(hk.mods, MOD_CONTROL | MOD_ALT);
        assert_eq!(hk.vk, b'T' as u32);
    }

    #[test]
    fn reject_empty_and_unknown() {
        assert!(parse_hotkey("").is_none());
        assert!(parse_hotkey("Foo+F1").is_none());
        assert!(parse_hotkey("F99").is_none());
    }
}
