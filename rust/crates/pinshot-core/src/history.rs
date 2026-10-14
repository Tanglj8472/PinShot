//! Snip history: full-desktop frames + selection rects. `,` / `.` step; newest last.
//! Last entries are also persisted under `data_dir/history` so `R` works after a restart.

use crate::geom::Rect;
use crate::raster::Raster;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub rect: Rect,
    pub image: Raster,
    /// On-disk PNG so the full-desktop raster can be dropped while idle.
    pub disk: Option<PathBuf>,
}

impl HistoryEntry {
    pub fn new(rect: Rect, image: Raster) -> Self {
        Self {
            rect,
            image,
            disk: None,
        }
    }

    pub fn resident(&self) -> bool {
        self.image.width >= 8 && self.image.height >= 8
    }

    pub fn evict_pixels(&mut self) {
        if self.image.width == 0 && self.image.height == 0 {
            return;
        }
        self.image = Raster::new(0, 0);
    }
}

#[derive(Clone, Debug, Default)]
pub struct SnipHistory {
    pub entries: Vec<HistoryEntry>,
    /// `entries.len()` means "live capture" (not a replay).
    pub idx: usize,
}

impl SnipHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, entry: HistoryEntry, max: u32) {
        let max = max.max(1) as usize;
        if let Some(last) = self.entries.last() {
            if last.rect == entry.rect {
                self.entries.pop();
            }
        }
        self.entries.push(entry);
        if self.entries.len() > max {
            let extra = self.entries.len() - max;
            self.entries.drain(0..extra);
        }
        self.idx = self.entries.len();
    }

    pub fn evict_pixels(&mut self) {
        for e in &mut self.entries {
            e.evict_pixels();
        }
    }

    pub fn last(&self) -> Option<&HistoryEntry> {
        self.entries.last()
    }

    /// Step `-1` (`,`) or `+1` (`.`). Returns the replay entry, or `None` for live.
    pub fn step(&mut self, dir: i32) -> HistoryView<'_> {
        if self.entries.is_empty() {
            return HistoryView::Live;
        }
        let max = self.entries.len();
        let next = (self.idx as i32 + dir).clamp(0, max as i32) as usize;
        self.idx = next;
        if self.idx == max {
            HistoryView::Live
        } else {
            HistoryView::Replay(&self.entries[self.idx])
        }
    }

    pub fn is_live(&self) -> bool {
        self.idx >= self.entries.len()
    }

    pub fn to_file(&self, names: &[String]) -> HistoryFile {
        HistoryFile {
            entries: self
                .entries
                .iter()
                .zip(names.iter())
                .map(|(e, file)| HistoryMetaEntry {
                    file: file.clone(),
                    x: e.rect.x,
                    y: e.rect.y,
                    w: e.rect.w,
                    h: e.rect.h,
                })
                .collect(),
        }
    }
}

/// On-disk sidecar for last snips (PNG files live next to `history.toml`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HistoryFile {
    #[serde(default)]
    pub entries: Vec<HistoryMetaEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryMetaEntry {
    pub file: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl HistoryMetaEntry {
    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, self.h)
    }
}

impl HistoryFile {
    pub fn dir(data: &Path) -> PathBuf {
        data.join("history")
    }

    pub fn path(data: &Path) -> PathBuf {
        Self::dir(data).join("history.toml")
    }

    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_toml(s: &str) -> Result<Self, String> {
        toml::from_str(s).map_err(|e| e.to_string())
    }
}

#[derive(Clone, Copy, Debug)]
pub enum HistoryView<'a> {
    Live,
    Replay(&'a HistoryEntry),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(x: f32) -> HistoryEntry {
        HistoryEntry::new(Rect::new(x, 0.0, 10.0, 10.0), Raster::new(4, 4))
    }

    #[test]
    fn push_caps_and_replaces_same_rect() {
        let mut h = SnipHistory::new();
        h.push(entry(1.0), 2);
        h.push(entry(1.0), 2);
        assert_eq!(h.entries.len(), 1);
        h.push(entry(2.0), 2);
        h.push(entry(3.0), 2);
        assert_eq!(h.entries.len(), 2);
        assert_eq!(h.entries[0].rect.x, 2.0);
        assert_eq!(h.idx, 2);
    }

    #[test]
    fn comma_dot_then_back_to_live() {
        let mut h = SnipHistory::new();
        h.push(entry(1.0), 10);
        h.push(entry(2.0), 10);
        match h.step(-1) {
            HistoryView::Replay(e) => assert_eq!(e.rect.x, 2.0),
            HistoryView::Live => panic!("expected replay"),
        }
        match h.step(-1) {
            HistoryView::Replay(e) => assert_eq!(e.rect.x, 1.0),
            HistoryView::Live => panic!("expected replay"),
        }
        assert!(matches!(h.step(-1), HistoryView::Replay(_)));
        assert!(matches!(h.step(1), HistoryView::Replay(_)));
        assert!(matches!(h.step(1), HistoryView::Live));
    }

    #[test]
    fn history_file_toml_roundtrip() {
        let f = HistoryFile {
            entries: vec![HistoryMetaEntry {
                file: "hist-000.png".into(),
                x: 10.0,
                y: 20.0,
                w: 100.0,
                h: 80.0,
            }],
        };
        let back = HistoryFile::from_toml(&f.to_toml().unwrap()).unwrap();
        assert_eq!(back.entries[0].rect(), Rect::new(10.0, 20.0, 100.0, 80.0));
        assert_eq!(HistoryFile::path(Path::new("/tmp/ps")).file_name().unwrap(), "history.toml");
    }

    #[test]
    fn evict_pixels_drops_resident_rasters() {
        let mut h = SnipHistory::new();
        h.push(entry(1.0), 10);
        assert!(h.entries[0].image.width > 0);
        h.evict_pixels();
        assert_eq!(h.entries[0].image.width, 0);
        assert_eq!(h.entries[0].rect.x, 1.0);
    }
}
