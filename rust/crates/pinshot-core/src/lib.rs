//! PinShot core: geometry, annotation, undo, i18n, config, icons, hooks.
//!
//! This crate is portable (Linux CI unit tests + Windows app). Windowing lives in `pinshot`.

pub mod annotation;
pub mod color;
pub mod colorpicker;
pub mod config;
pub mod confirm;
pub mod cursor;
pub mod font;
pub mod geom;
pub mod groups;
pub mod history;
pub mod hooks;
pub mod hotkey;
pub mod i18n;
pub mod icons;
pub mod magnifier;
pub mod ocr;
pub mod raster;
pub mod render;
pub mod selection;
pub mod session;
pub mod snap;
pub mod translate;
pub mod wheel;

pub use annotation::{
    Annotator, ArrowKind, DrawMode, Effect, LineStyleKind, Shape, Style, Tool, UndoStack,
};
pub use color::{hsv_to_rgb, palette_colors, parse_color_text, rgb_to_hsv, Rgba, PALETTE};
pub use colorpicker::{
    color_picker_layout, ColorPicker, ColorPickerLayout, PickerAction, PickerHit,
};
pub use config::{config_dir, config_path, data_dir, Config, ConfigError, TranslateSettings};
pub use confirm::{confirm_abort_layout, ConfirmAbortHit, ConfirmAbortLayout};
pub use cursor::{cursor_for, CursorHint, CursorKind};
pub use geom::{
    hover_rect, overlay_from_screen, phys_to_logical, place_near_cursor, screen_from_overlay,
    Handle, Point, Rect,
};
pub use groups::{label as group_label, DEFAULT_GROUP};
pub use history::{HistoryEntry, HistoryFile, HistoryMetaEntry, HistoryView, SnipHistory};
pub use hooks::{
    ElementDetector, ElementDetectorStub, OcrEngine, OcrStub, SettingsUi, SettingsUiStub,
    Translator, TranslatorStub,
};
pub use hotkey::{bindings_from_config, find_conflicts, parse_hotkey, Binding, Command, Hotkey};
pub use i18n::{lang, set_lang, set_lang_code, tr, Lang};
pub use icons::{app_icon_rgba, draw_app_icon, draw_icon, IconKind};
pub use magnifier::{layout_magnifier, magnifier_visible, sample_color, Magnifier};
pub use ocr::OcrModelKind;
pub use raster::Raster;
pub use selection::{DetectMode, Nudge, PressKind, SelState, Selection};
pub use session::{ClosedPin, ClosedStack, PinSnapshot, SessionFile};
pub use snap::{SnapGuides, SNAP_PX};
pub use wheel::WheelGate;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
