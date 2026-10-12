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

/// UI language → Edge `to=` code.
pub fn target_from_ui_lang(lang: &str) -> &'static str {
    match lang {
        "en" => "en",
        _ => "zh-Hans",
    }
}

/// `zh-Hans` / `zh-Hant` / `zh` → `zh`; `en-US` → `en`.
pub fn lang_family(code: &str) -> &str {
    let c = code.trim();
    if c.len() >= 2 {
        match &c.as_bytes()[..2] {
            b"zh" => return "zh",
            b"ja" => return "ja",
            b"en" => return "en",
            _ => {}
        }
    }
    c
}

/// Heuristic source family for a snippet (CJK vs Kana vs Latin).
pub fn detect_lang_family(text: &str) -> &'static str {
    let mut cjk = 0u32;
    let mut kana = 0u32;
    let mut latin = 0u32;
    for ch in text.chars() {
        match ch {
            '\u{3040}'..='\u{30FF}' => kana += 1,
            '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' => cjk += 1,
            c if c.is_ascii_alphabetic() => latin += 1,
            _ => {}
        }
    }
    if kana > cjk && kana > latin {
        "ja"
    } else if cjk >= latin && cjk > 0 {
        "zh"
    } else {
        "en"
    }
}

/// If the source is already the configured target, swap like common translators
/// (Chinese → English; English → Simplified Chinese).
pub fn effective_target(text: &str, configured: &str) -> String {
    let src = detect_lang_family(text);
    let tgt = lang_family(configured);
    if src == tgt {
        if tgt == "en" {
            "zh-Hans".into()
        } else {
            "en".into()
        }
    } else {
        configured.to_string()
    }
}

pub fn request_body(text: &str) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("No text to translate".into());
    }
    serde_json::to_string(&[text]).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct Item {
    #[serde(rename = "detectedLanguage")]
    detected_language: Option<Detected>,
    translations: Option<Vec<Part>>,
}

#[derive(Deserialize)]
struct Detected {
    language: Option<String>,
}

#[derive(Deserialize)]
struct Part {
    text: Option<String>,
}

pub struct Translation {
    pub text: String,
    pub detected: Option<String>,
}

/// Parse Edge `translatetext` JSON (array of `{ translations: [{ text }] }`).
pub fn read_translation(body: &str) -> Result<String, String> {
    Ok(read_translation_ex(body)?.text)
}

pub fn read_translation_ex(body: &str) -> Result<Translation, String> {
    let data: Vec<Item> = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let mut parts = Vec::new();
    let mut detected = None;
    for item in data {
        if detected.is_none() {
            detected = item
                .detected_language
                .as_ref()
                .and_then(|d| d.language.clone());
        }
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
    Ok(Translation {
        text: parts.join("\n"),
        detected,
    })
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

    #[test]
    fn chinese_source_swaps_zh_target_to_english() {
        assert_eq!(effective_target("並且在2010年首次發布。", "zh-Hans"), "en");
        assert_eq!(effective_target("并且在2010年首次发布。", "zh-Hans"), "en");
        assert_eq!(
            effective_target("In 2009, Mozilla sponsored the project.", "zh-Hans"),
            "zh-Hans"
        );
        assert_eq!(
            effective_target("In 2009, Mozilla sponsored the project.", "en"),
            "zh-Hans"
        );
        assert_eq!(target_from_ui_lang("zh_CN"), "zh-Hans");
        assert_eq!(target_from_ui_lang("en"), "en");
    }

    #[test]
    fn parse_keeps_detected_language() {
        let body = r#"[{"detectedLanguage":{"language":"zh-Hant","score":1.0},"translations":[{"text":"Hello","to":"en"}]}]"#;
        let t = read_translation_ex(body).unwrap();
        assert_eq!(t.text, "Hello");
        assert_eq!(t.detected.as_deref(), Some("zh-Hant"));
    }
}
