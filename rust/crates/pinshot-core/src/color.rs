//! RGBA colour + the 20-swatch annotation palette.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const BLUE: Self = Self {
        r: 30,
        g: 144,
        b: 255,
        a: 255,
    };
    pub const SELECTION: Self = Self::BLUE;
    pub const RED: Self = Self {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const MARKER_YELLOW: Self = Self {
        r: 255,
        g: 212,
        b: 0,
        a: 255,
    };

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        match s.len() {
            3 => {
                let r = u8::from_str_radix(&s[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&s[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&s[2..3].repeat(2), 16).ok()?;
                Some(Self::rgb(r, g, b))
            }
            6 => {
                let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                Some(Self::rgb(r, g, b))
            }
            8 => {
                let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                let a = u8::from_str_radix(&s[6..8], 16).ok()?;
                Some(Self::rgba(r, g, b, a))
            }
            12 => {
                // zenity --color-selection often prints 16-bit #RRRRGGGGBBBB
                let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                let g = u8::from_str_radix(&s[4..6], 16).ok()?;
                let b = u8::from_str_radix(&s[8..10], 16).ok()?;
                Some(Self::rgb(r, g, b))
            }
            _ => None,
        }
    }

    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    pub fn rgb_text(self) -> String {
        format!("{}, {}, {}", self.r, self.g, self.b)
    }

    pub fn format(self, hex: bool) -> String {
        if hex {
            self.hex()
        } else {
            self.rgb_text()
        }
    }

    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    pub fn lightness(self) -> u8 {
        // Rec. 601-ish lightness used by the Python annotator.
        ((self.r as u16 * 30 + self.g as u16 * 59 + self.b as u16 * 11) / 100) as u8
    }

    pub fn darker(self) -> Self {
        Self {
            r: self.r.saturating_mul(4) / 5,
            g: self.g.saturating_mul(4) / 5,
            b: self.b.saturating_mul(4) / 5,
            a: self.a,
        }
    }

    pub fn to_skia(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.r, self.g, self.b, self.a)
    }

    pub fn to_u32_xrgb(self) -> u32 {
        (self.r as u32) << 16 | (self.g as u32) << 8 | self.b as u32
    }
}

/// Same 20-swatch palette as `pinshot/annotate.py`.
pub const PALETTE: [&str; 20] = [
    "#ff0000", "#ff8800", "#ffd400", "#55cc00", "#00aa55", "#00c8c8", "#0088ff", "#2040ff",
    "#8830ff", "#ff40c0", "#ffffff", "#c0c0c0", "#808080", "#404040", "#000000", "#ffb0b0",
    "#ffe0a0", "#b0f0b0", "#a0d8ff", "#e0b0ff",
];

pub fn palette_colors() -> [Rgba; 20] {
    let mut out = [Rgba::RED; 20];
    for (i, h) in PALETTE.iter().enumerate() {
        out[i] = Rgba::from_hex(h).unwrap_or(Rgba::RED);
    }
    out
}

/// Parse a colour string from the clipboard (`#ff8800`, `#f80`, `255,136,0`, `rgb(...)`).
pub fn parse_color_text(s: &str) -> Option<Rgba> {
    let s = s.trim();
    if let Some(c) = Rgba::from_hex(s) {
        return Some(c);
    }
    let inner = s
        .strip_prefix("rgb(")
        .and_then(|t| t.strip_suffix(')'))
        .unwrap_or(s);
    let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
    if parts.len() == 3 {
        let r = parts[0].parse().ok()?;
        let g = parts[1].parse().ok()?;
        let b = parts[2].parse().ok()?;
        return Some(Rgba::rgb(r, g, b));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let c = Rgba::from_hex("#1e90ff").unwrap();
        assert_eq!(c, Rgba::BLUE);
        assert_eq!(c.hex(), "#1E90FF");
        assert_eq!(Rgba::from_hex("#f80").unwrap(), Rgba::from_hex("#ff8800").unwrap());
        assert_eq!(Rgba::from_hex("#ffff88000000").unwrap(), Rgba::from_hex("#ff8800").unwrap());
    }

    #[test]
    fn format_rgb_hex() {
        let c = Rgba::rgb(255, 136, 0);
        assert_eq!(c.format(false), "255, 136, 0");
        assert_eq!(c.format(true), "#FF8800");
    }

    #[test]
    fn palette_has_twenty() {
        assert_eq!(palette_colors().len(), 20);
        assert_eq!(palette_colors()[0], Rgba::RED);
    }

    #[test]
    fn parse_clipboard_colors() {
        assert!(parse_color_text("#ff8800").is_some());
        assert!(parse_color_text("255,136,0").is_some());
        assert!(parse_color_text("rgb(255, 136, 0)").is_some());
        assert!(parse_color_text("not a color").is_none());
    }
}
