//! Persist open pins + groups as PNG blobs + `session.toml`.

use crate::annotation::Shape;
use crate::geom::Point;
use crate::groups;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PinSnapshot {
    pub file: String,
    pub x: i32,
    pub y: i32,
    pub scale: f32,
    pub opacity: f32,
    pub group: String,
    #[serde(default)]
    pub shapes: Vec<Shape>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct SessionFile {
    pub groups: Vec<String>,
    pub pins: Vec<PinSnapshot>,
}

#[derive(Clone, Debug)]
pub struct ClosedPin {
    pub png: Vec<u8>,
    pub pos: (i32, i32),
    pub scale: f32,
    pub opacity: f32,
    pub group: String,
}

#[derive(Clone, Debug, Default)]
pub struct ClosedStack {
    items: Vec<ClosedPin>,
}

impl ClosedStack {
    pub fn push(&mut self, pin: ClosedPin, max: u32) {
        if max == 0 {
            return;
        }
        self.items.push(pin);
        let max = max as usize;
        if self.items.len() > max {
            let extra = self.items.len() - max;
            self.items.drain(0..extra);
        }
    }

    pub fn pop(&mut self) -> Option<ClosedPin> {
        self.items.pop()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
}

pub fn session_path(dir: &Path) -> PathBuf {
    dir.join("session.toml")
}

pub fn pin_png_path(dir: &Path, file: &str) -> PathBuf {
    dir.join("session").join(file)
}

impl SessionFile {
    pub fn sanitize(mut self) -> Self {
        self.groups = groups::dedupe(&self.groups);
        for p in &mut self.pins {
            p.group = groups::find(&self.groups, &p.group)
                .unwrap_or(groups::DEFAULT)
                .to_string();
            p.scale = p.scale.clamp(0.05, 8.0);
            p.opacity = p.opacity.clamp(0.1, 1.0);
        }
        self
    }

    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_toml(s: &str) -> Result<Self, String> {
        let f: SessionFile = toml::from_str(s).map_err(|e| e.to_string())?;
        Ok(f.sanitize())
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        fs::create_dir_all(dir.join("session")).map_err(|e| e.to_string())?;
        fs::write(session_path(dir), self.to_toml()?).map_err(|e| e.to_string())
    }

    pub fn load(dir: &Path) -> Result<Self, String> {
        let p = session_path(dir);
        if !p.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(p).map_err(|e| e.to_string())?;
        Self::from_toml(&text)
    }
}

pub fn next_pin_filename(existing: &[PinSnapshot]) -> String {
    let mut n = existing.len();
    loop {
        let name = format!("pin-{n:03}.png");
        if !existing.iter().any(|p| p.file == name) {
            return name;
        }
        n += 1;
    }
}

pub fn pin_origin(pos: Point) -> (i32, i32) {
    (pos.x.round() as i32, pos.y.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_roundtrip() {
        let s = SessionFile {
            groups: vec!["Default".into(), "Work".into()],
            pins: vec![PinSnapshot {
                file: "pin-000.png".into(),
                x: 10,
                y: 20,
                scale: 1.5,
                opacity: 0.8,
                group: "Work".into(),
                shapes: Vec::new(),
            }],
        };
        let back = SessionFile::from_toml(&s.to_toml().unwrap()).unwrap();
        assert_eq!(back.pins[0].group, "Work");
        assert_eq!(back.pins[0].scale, 1.5);
    }

    #[test]
    fn shapes_survive_toml_roundtrip() {
        use crate::annotation::{Shape, Style, Tool};
        let sh = Shape::new(
            Tool::Line,
            vec![Point::new(1.0, 2.0), Point::new(8.0, 9.0)],
            Style::default(),
        );
        let s = SessionFile {
            groups: vec!["Default".into()],
            pins: vec![PinSnapshot {
                file: "pin-000.png".into(),
                x: 1,
                y: 2,
                scale: 1.0,
                opacity: 1.0,
                group: "Default".into(),
                shapes: vec![sh.clone()],
            }],
        };
        let back = SessionFile::from_toml(&s.to_toml().unwrap()).unwrap();
        assert_eq!(back.pins[0].shapes.len(), 1);
        assert_eq!(back.pins[0].shapes[0].kind, Tool::Line);
        assert_eq!(back.pins[0].shapes[0].pts[1], Point::new(8.0, 9.0));
    }

    #[test]
    fn closed_stack_caps() {
        let mut st = ClosedStack::default();
        for i in 0..5 {
            st.push(
                ClosedPin {
                    png: vec![i],
                    pos: (0, 0),
                    scale: 1.0,
                    opacity: 1.0,
                    group: "Default".into(),
                },
                2,
            );
        }
        assert_eq!(st.len(), 2);
        assert_eq!(st.pop().unwrap().png, vec![4]);
    }

    #[test]
    fn missing_session_is_empty() {
        let dir = std::env::temp_dir().join("pinshot-no-session-dir");
        let _ = fs::remove_dir_all(&dir);
        let s = SessionFile::load(&dir).unwrap();
        assert!(s.pins.is_empty());
    }
}
