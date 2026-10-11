//! PinShot core: geometry, annotation, undo, i18n, config, icons, hooks.
//!
//! This crate is portable (Linux CI unit tests + Windows app). Windowing lives in `pinshot`.

pub mod annotation;
pub mod color;
pub mod config;
pub mod geom;
pub mod hooks;
pub mod hotkey;
pub mod i18n;
pub mod icons;
pub mod magnifier;
pub mod raster;
pub mod render;
pub mod selection;

pub use annotation::{Annotator, Effect, Shape, Style, Tool, UndoStack};
pub use color::{parse_color_text, palette_colors, Rgba, PALETTE};
pub use config::{config_dir, config_path, Config, ConfigError};
pub use geom::{hover_rect, phys_to_logical, Handle, Point, Rect};
pub use hooks::{
    ElementDetector, ElementDetectorStub, OcrEngine, OcrStub, SettingsUi, SettingsUiStub,
    Translator, TranslatorStub,
};
pub use hotkey::{parse_hotkey, Command, Hotkey};
pub use i18n::{lang, set_lang, set_lang_code, tr, Lang};
pub use icons::{app_icon_rgba, draw_app_icon, draw_icon, IconKind};
pub use magnifier::{layout_magnifier, magnifier_visible, sample_color, Magnifier};
pub use raster::Raster;
pub use selection::{DetectMode, Nudge, PressKind, SelState, Selection};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
