//! No translation network requests are compiled into the Lite host.
use pinshot_core::config::TranslateSettings;

pub fn translate_with(_: &TranslateSettings, _: &str, _: &str) -> Result<String, String> {
    Err("Translation is not included in PinShot Lite".into())
}
pub fn translate_with_opts(_: &TranslateSettings, _: &str, _: &str, _: bool) -> Result<String, String> {
    Err("Translation is not included in PinShot Lite".into())
}
pub fn list_models(_: &TranslateSettings) -> Result<Vec<String>, String> {
    Err("Translation is not included in PinShot Lite".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_translation_requests_are_disabled() {
        let settings = TranslateSettings::default();
        assert!(translate_with(&settings, "zh_CN", "hello").is_err());
        assert!(translate_with_opts(&settings, "en", "hello", true).is_err());
        assert!(list_models(&settings).is_err());
    }
}
