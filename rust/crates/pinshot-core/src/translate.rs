//! Microsoft Edge translation endpoint (same as Python `translation.py`).

use serde::Deserialize;

pub const EDGE_URL: &str = "https://edge.microsoft.com/translate/translatetext";

pub const TARGETS: &[(&str, &str)] = &[
    ("简体中文", "zh-Hans"),
    ("English", "en"),
    ("日本語", "ja"),
    ("繁體中文", "zh-Hant"),
];

pub fn request_url(target: &str) -> String {
    format!("{EDGE_URL}?from=&to={target}&isEnterpriseClient=false")
}

pub fn request_body(text: &str) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("No text to translate".into());
    }
    serde_json::to_string(&[text]).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct Item {
    translations: Option<Vec<Part>>,
}

#[derive(Deserialize)]
struct Part {
    text: Option<String>,
}

/// Parse Edge `translatetext` JSON (array of `{ translations: [{ text }] }`).
pub fn read_translation(body: &str) -> Result<String, String> {
    let data: Vec<Item> = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let mut parts = Vec::new();
    for item in data {
        for t in item.translations.unwrap_or_default() {
            if let Some(text) = t.text {
                if !text.is_empty() {
                    parts.push(text);
                }
            }
        }
    }
    if parts.is_empty() {
        return Err("No translation returned".into());
    }
    Ok(parts.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_edge_payload() {
        let body = r#"[{"detectedLanguage":{"language":"en","score":1.0},"translations":[{"text":"你好","to":"zh-Hans"}]}]"#;
        assert_eq!(read_translation(body).unwrap(), "你好");
    }

    #[test]
    fn empty_text_rejected() {
        assert!(request_body("   ").is_err());
    }

    #[test]
    fn url_includes_target() {
        assert!(request_url("zh-Hans").contains("to=zh-Hans"));
    }
}
