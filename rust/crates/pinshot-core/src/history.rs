//! Snip history: full-desktop frames + selection rects. `,` / `.` step; newest last.

use crate::geom::Rect;
use crate::raster::Raster;

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub rect: Rect,
    pub image: Raster,
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
        HistoryEntry {
            rect: Rect::new(x, 0.0, 10.0, 10.0),
            image: Raster::new(4, 4),
        }
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
}
