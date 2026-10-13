//! PinShot settings. Stored as TOML under `%APPDATA%\PinShot` (Windows).

use crate::i18n::Lang;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schema_version: u32,
    pub language: String,
    pub start_minimized: bool,
    pub skip_abort_confirm: bool,
    pub dblclick_pin_action: String,
    pub dblclick_tip_shown: bool,
    pub hotkey_snip: String,
    pub hotkey_pin: String,
    pub hotkey_hide_all: String,
    pub hotkey_clickthrough: String,
    pub hotkey_restore: String,
    pub hotkey_selection_translate: String,
    pub hotkey_capture_translate: String,
    pub run_at_login: bool,
    pub groups: Vec<String>,
    pub quick_save_folder: String,
    pub image_quality: u8,
    pub snip_delay: u32,
    pub history_max: u32,
    pub closed_pins_max: u32,
    pub theme: String,
    pub dim_alpha: u8,
    /// `mobile` (default, faster) or `server` (~170 MB, more accurate).
    #[serde(default)]
    pub ocr_model: String,
    /// F5 translator (pot / STranslate / openai-translator style).
    #[serde(default)]
    pub translate: TranslateSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            language: "zh_CN".into(),
            start_minimized: true,
            skip_abort_confirm: false,
            dblclick_pin_action: "close".into(),
            dblclick_tip_shown: false,
            hotkey_snip: "F1".into(),
            hotkey_pin: "F3".into(),
            hotkey_hide_all: "Shift+F3".into(),
            hotkey_clickthrough: "F4".into(),
            hotkey_restore: "Ctrl+Shift+Z".into(),
            hotkey_selection_translate: "F5".into(),
            hotkey_capture_translate: "F6".into(),
            run_at_login: false,
            groups: vec![
                "Default".into(),
                "Group 1".into(),
                "Group 2".into(),
                "Group 3".into(),
            ],
            quick_save_folder: String::new(),
            image_quality: 92,
            snip_delay: 0,
            history_max: 10,
            closed_pins_max: 10,
            theme: "system".into(),
            dim_alpha: 110,
            ocr_model: "mobile".into(),
            translate: TranslateSettings::default(),
        }
    }
}

/// Provider + endpoint + key + model + target. `api_key` is never shown in Debug.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TranslateSettings {
    /// `edge` (free default), `openai` (chat-completions), or `deepl`.
    pub provider: String,
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    /// Edge/DeepL/OpenAI `to=` code (`en`, `zh-Hans`, …). Empty = follow UI language.
    pub target: String,
}

impl Default for TranslateSettings {
    fn default() -> Self {
        Self {
            provider: "edge".into(),
            api_url: String::new(),
            api_key: String::new(),
            model: String::new(),
            target: String::new(),
        }
    }
}

impl std::fmt::Debug for TranslateSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TranslateSettings")
            .field("provider", &self.provider)
            .field("api_url", &self.api_url)
            .field("api_key", &if self.api_key.is_empty() { "" } else { "***" })
            .field("model", &self.model)
            .field("target", &self.target)
            .finish()
    }
}

impl TranslateSettings {
    pub fn normalized_provider(&self) -> &str {
        match self.provider.trim() {
            "openai" | "openai-compatible" | "openai_compatible" => "openai",
            "deepl" => "deepl",
            _ => "edge",
        }
    }
}

impl Config {
    pub fn lang(&self) -> Lang {
        Lang::from_code(&self.language)
    }

    pub fn validate(mut self) -> Self {
        if Lang::from_code(&self.language) == Lang::ZhCn
            && self.language != "zh_CN"
            && self.language != "en"
        {
            self.language = "zh_CN".into();
        }
        if !matches!(self.language.as_str(), "en" | "zh_CN") {
            // from_code maps unknown → zh, keep a canonical code
            self.language = self.lang().code().into();
        }
        if !matches!(self.dblclick_pin_action.as_str(), "close" | "hide" | "none") {
            self.dblclick_pin_action = "close".into();
        }
        self.image_quality = self.image_quality.clamp(1, 100);
        self.history_max = self.history_max.max(1);
        self.closed_pins_max = self.closed_pins_max.max(1);
        self.dim_alpha = self.dim_alpha.min(200);
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            self.theme = "system".into();
        }
        self.ocr_model = match self.ocr_model.trim() {
            "server" => "server".into(),
            _ => "mobile".into(),
        };
        self.groups = crate::groups::dedupe(&self.groups);
        if self.hotkey_selection_translate.trim().is_empty() {
            self.hotkey_selection_translate = "F5".into();
        }
        if self.hotkey_capture_translate.trim().is_empty() {
            self.hotkey_capture_translate = "F6".into();
        }
        if self.hotkey_clickthrough.trim().is_empty() {
            self.hotkey_clickthrough = "F4".into();
        }
        if self.schema_version == 0 {
            self.schema_version = SCHEMA_VERSION;
        }
        match self.translate.provider.trim() {
            "openai" | "openai-compatible" | "openai_compatible" => {
                self.translate.provider = "openai".into();
            }
            "deepl" => self.translate.provider = "deepl".into(),
            _ => self.translate.provider = "edge".into(),
        }
        self.translate.api_url = self.translate.api_url.trim().to_string();
        self.translate.api_key = self.translate.api_key.trim().to_string();
        self.translate.model = self.translate.model.trim().to_string();
        self.translate.target = self.translate.target.trim().to_string();
        self
    }

    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(self).map_err(|e| ConfigError::Serialize(e.to_string()))
    }

    pub fn from_toml(s: &str) -> Result<Self, ConfigError> {
        let cfg: Config = toml::from_str(s).map_err(|e| ConfigError::Parse(e.to_string()))?;
        Ok(cfg.validate())
    }

    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path).map_err(|e| ConfigError::Io(e.to_string()))?;
        Self::from_toml(&text)
    }

    pub fn save_to(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| ConfigError::Io(e.to_string()))?;
        }
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, self.to_toml()?).map_err(|e| ConfigError::Io(e.to_string()))?;
        fs::rename(&tmp, path).map_err(|e| ConfigError::Io(e.to_string()))?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("config io: {0}")]
    Io(String),
    #[error("config parse: {0}")]
    Parse(String),
    #[error("config serialize: {0}")]
    Serialize(String),
}

/// `%APPDATA%\PinShot` on Windows; `$XDG_CONFIG_HOME/PinShot` elsewhere (tests).
pub fn config_dir() -> PathBuf {
    if let Ok(p) = std::env::var("PINSHOT_CONFIG_DIR") {
        return PathBuf::from(p);
    }
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("PinShot");
        }
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("PinShot");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config").join("PinShot");
    }
    PathBuf::from("PinShot")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// `%APPDATA%\PinShot` on Windows; `$XDG_DATA_HOME/PinShot` or `~/.local/share/PinShot` elsewhere.
/// Session PNGs, OCR models, and the debug log live here. Config stays in [`config_dir`].
pub fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("PINSHOT_DATA_DIR") {
        return PathBuf::from(p);
    }
    #[cfg(windows)]
    {
        return config_dir();
    }
    #[cfg(not(windows))]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join("PinShot");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("PinShot");
        }
        PathBuf::from("PinShot")
    }
}

/// `~/.config/autostart` on Linux; unused on Windows (HKCU Run).
pub fn xdg_autostart_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("autostart");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config").join("autostart");
    }
    PathBuf::from("autostart")
}

pub fn load() -> Result<Config, ConfigError> {
    Config::load_from(&config_path())
}

pub fn save(cfg: &Config) -> Result<(), ConfigError> {
    cfg.save_to(&config_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);

    fn temp_path() -> PathBuf {
        let n = N.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("pinshot-core-cfg-{n}.toml"))
    }

    #[test]
    fn defaults_are_zh() {
        let c = Config::default();
        assert_eq!(c.language, "zh_CN");
        assert_eq!(c.hotkey_snip, "F1");
        assert_eq!(c.hotkey_pin, "F3");
        assert_eq!(c.hotkey_clickthrough, "F4");
        assert_eq!(c.schema_version, 1);
    }

    #[test]
    fn toml_roundtrip() {
        let mut c = Config::default();
        c.skip_abort_confirm = true;
        c.image_quality = 80;
        c.quick_save_folder = r"D:\Pics".into();
        let text = c.to_toml().unwrap();
        assert!(text.contains("skip_abort_confirm"));
        let back = Config::from_toml(&text).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn validate_clamps() {
        let mut c = Config::default();
        c.image_quality = 0;
        c.history_max = 0;
        c.dblclick_pin_action = "explode".into();
        c = c.validate();
        assert_eq!(c.image_quality, 1);
        assert_eq!(c.history_max, 1);
        assert_eq!(c.dblclick_pin_action, "close");
    }

    #[test]
    fn file_roundtrip() {
        let path = temp_path();
        let mut c = Config::default();
        c.language = "en".into();
        c.save_to(&path).unwrap();
        let back = Config::load_from(&path).unwrap();
        assert_eq!(back.language, "en");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn missing_file_is_default() {
        let path = std::env::temp_dir().join("pinshot-core-missing-nope.toml");
        let _ = fs::remove_file(&path);
        let c = Config::load_from(&path).unwrap();
        assert_eq!(c, Config::default());
    }

    #[test]
    fn bad_toml_errors() {
        assert!(Config::from_toml("language = [").is_err());
    }

    #[test]
    fn data_dir_honors_env() {
        let p = std::env::temp_dir().join("pinshot-core-data-test");
        std::env::set_var("PINSHOT_DATA_DIR", &p);
        assert_eq!(data_dir(), p);
        std::env::remove_var("PINSHOT_DATA_DIR");
    }

    #[test]
    fn old_toml_fills_new_fields() {
        let c = Config::from_toml("language = \"en\"\nhotkey_snip = \"F2\"\n").unwrap();
        assert_eq!(c.language, "en");
        assert_eq!(c.hotkey_snip, "F2");
        assert_eq!(c.hotkey_selection_translate, "F5");
        assert_eq!(c.hotkey_clickthrough, "F4");
        assert_eq!(c.ocr_model, "mobile");
        assert!(c.groups.iter().any(|g| g == "Default"));
    }

    #[test]
    fn empty_clickthrough_becomes_f4() {
        let c = Config::from_toml("hotkey_clickthrough = \"\"\n").unwrap();
        assert_eq!(c.hotkey_clickthrough, "F4");
    }

    #[test]
    fn translate_settings_roundtrip_redacts_key() {
        let mut c = Config::default();
        c.translate.provider = "openai".into();
        c.translate.api_url = "https://api.deepseek.com/v1/chat/completions".into();
        c.translate.api_key = "sk-secret-do-not-log".into();
        c.translate.model = "deepseek-chat".into();
        c.translate.target = "en".into();
        let text = c.to_toml().unwrap();
        assert!(text.contains("[translate]"));
        assert!(text.contains("sk-secret-do-not-log"), "key is stored");
        let back = Config::from_toml(&text).unwrap();
        assert_eq!(back.translate.api_key, "sk-secret-do-not-log");
        assert_eq!(back.translate.provider, "openai");
        let dbg = format!("{:?}", c.translate);
        assert!(!dbg.contains("sk-secret"));
        assert!(dbg.contains("***"));
    }

    #[test]
    fn old_toml_defaults_translate_to_edge() {
        let c = Config::from_toml("language = \"en\"\n").unwrap();
        assert_eq!(c.translate.provider, "edge");
        assert!(c.translate.api_key.is_empty());
    }
}
