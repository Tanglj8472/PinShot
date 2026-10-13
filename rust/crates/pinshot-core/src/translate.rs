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

pub const OPENAI_CHAT_DEFAULT: &str = "https://api.openai.com/v1/chat/completions";
pub const DEEPL_URL_DEFAULT: &str = "https://api-free.deepl.com/v2/translate";
pub const OPENAI_MODEL_DEFAULT: &str = "gpt-4o-mini";

/// Public OpenAI-compatible bases (Pot / STranslate service list style).
pub const OPENAI_URL_DEFAULT: &str = "https://api.openai.com/v1";
pub const DEEPSEEK_URL_DEFAULT: &str = "https://api.deepseek.com";
pub const DEEPSEEK_MODEL_DEFAULT: &str = "deepseek-chat";
pub const QWEN_URL_DEFAULT: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
pub const QWEN_MODEL_DEFAULT: &str = "qwen-turbo";
pub const OLLAMA_URL_DEFAULT: &str = "http://127.0.0.1:11434/v1";
pub const OLLAMA_MODEL_DEFAULT: &str = "llama3.2";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServicePreset {
    pub id: &'static str,
    pub default_url: &'static str,
    pub default_model: &'static str,
}

pub const COMPAT_PRESETS: &[ServicePreset] = &[
    ServicePreset {
        id: "openai",
        default_url: OPENAI_URL_DEFAULT,
        default_model: OPENAI_MODEL_DEFAULT,
    },
    ServicePreset {
        id: "deepseek",
        default_url: DEEPSEEK_URL_DEFAULT,
        default_model: DEEPSEEK_MODEL_DEFAULT,
    },
    ServicePreset {
        id: "qwen",
        default_url: QWEN_URL_DEFAULT,
        default_model: QWEN_MODEL_DEFAULT,
    },
    ServicePreset {
        id: "ollama",
        default_url: OLLAMA_URL_DEFAULT,
        default_model: OLLAMA_MODEL_DEFAULT,
    },
];

pub fn service_preset(id: &str) -> Option<&'static ServicePreset> {
    let id = match id.trim() {
        "tongyi" | "dashscope" => "qwen",
        "openai-compatible" | "openai_compatible" => "openai",
        other => other,
    };
    COMPAT_PRESETS.iter().find(|p| p.id == id)
}

/// Fill URL + model when the user picks a service (keeps the API key).
pub fn apply_service_defaults(settings: &mut crate::config::TranslateSettings) {
    let id = settings.normalized_provider().to_string();
    settings.provider = id.clone();
    match id.as_str() {
        "deepl" => {
            settings.api_url = DEEPL_URL_DEFAULT.into();
        }
        "edge" => {}
        _ => {
            if let Some(p) = service_preset(&id) {
                settings.api_url = p.default_url.into();
                settings.model = p.default_model.into();
            }
        }
    }
}

pub fn resolved_openai_url(settings: &crate::config::TranslateSettings) -> String {
    let raw = settings.api_url.trim();
    if raw.is_empty() {
        if let Some(p) = service_preset(settings.normalized_provider()) {
            return openai_chat_url(p.default_url);
        }
    }
    openai_chat_url(raw)
}

pub fn resolved_openai_model(settings: &crate::config::TranslateSettings) -> String {
    let t = settings.model.trim();
    if !t.is_empty() {
        return t.to_string();
    }
    service_preset(settings.normalized_provider())
        .map(|p| p.default_model.to_string())
        .unwrap_or_else(|| OPENAI_MODEL_DEFAULT.to_string())
}

/// pot-desktop: if the path is not already `/chat/completions`, append
/// `/v1/chat/completions` (covers OpenAI, DeepSeek, Qwen, Ollama, OpenRouter).
pub fn openai_chat_url(raw: &str) -> String {
    let mut u = raw.trim().to_string();
    if u.is_empty() {
        return OPENAI_CHAT_DEFAULT.into();
    }
    if !u.starts_with("http://") && !u.starts_with("https://") {
        u = format!("https://{u}");
    }
    while u.ends_with('/') {
        u.pop();
    }
    if u.ends_with("/chat/completions") {
        return u;
    }
    if u.ends_with("/v1") {
        return format!("{u}/chat/completions");
    }
    format!("{u}/v1/chat/completions")
}

pub fn openai_model(raw: &str) -> &str {
    let t = raw.trim();
    if t.is_empty() {
        OPENAI_MODEL_DEFAULT
    } else {
        t
    }
}

/// Human name for the prompt (`$to` in pot / openai-translator).
pub fn target_display_name(code: &str) -> &'static str {
    for (name, c) in TARGETS {
        if *c == code {
            return name;
        }
    }
    match lang_family(code) {
        "zh" => "简体中文",
        "ja" => "日本語",
        "en" => "English",
        _ => "English",
    }
}

pub fn openai_messages(text: &str, target: &str) -> serde_json::Value {
    let to = target_display_name(target);
    serde_json::json!([
        {
            "role": "system",
            "content": "You are a professional translation engine. Translate into a colloquial, professional, elegant and fluent result, without machine-translation style. Output only the translation, never interpret or add notes."
        },
        {
            "role": "user",
            "content": format!("Translate into {to}:\n\"\"\"\n{text}\n\"\"\"")
        }
    ])
}

pub fn openai_request_body(model: &str, text: &str, target: &str) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("No text to translate".into());
    }
    let body = serde_json::json!({
        "model": openai_model(model),
        "temperature": 0.2,
        "messages": openai_messages(text, target),
    });
    serde_json::to_string(&body).map_err(|e| e.to_string())
}

/// `choices[0].message.content` from chat-completions (string or text parts).
pub fn openai_read_content(body: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    if let Some(msg) = v
        .pointer("/error/message")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
    {
        return Err(msg.into());
    }
    let content = v
        .pointer("/choices/0/message/content")
        .ok_or_else(|| "No translation returned".to_string())?;
    let text = match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter_map(|p| {
                p.get("text")
                    .and_then(|t| t.as_str())
                    .or_else(|| p.as_str())
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    };
    let text = text.trim().to_string();
    if text.is_empty() {
        Err("No translation returned".into())
    } else {
        Ok(text)
    }
}

pub fn deepl_url(raw: &str) -> String {
    let u = raw.trim();
    if u.is_empty() {
        DEEPL_URL_DEFAULT.into()
    } else if u.starts_with("http://") || u.starts_with("https://") {
        u.trim_end_matches('/').to_string()
    } else {
        format!("https://{u}")
    }
}

pub fn deepl_target(code: &str) -> &'static str {
    match lang_family(code) {
        "zh" => "ZH",
        "ja" => "JA",
        _ => "EN",
    }
}

pub fn deepl_form_body(text: &str, target: &str) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("No text to translate".into());
    }
    Ok(format!(
        "text={}&target_lang={}",
        urlencoding_lite(text),
        deepl_target(target)
    ))
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn deepl_read_text(body: &str) -> Result<Translation, String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let text = v
        .pointer("/translations/0/text")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if text.is_empty() {
        return Err("No translation returned".into());
    }
    let detected = v
        .pointer("/translations/0/detected_source_language")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    Ok(Translation { text, detected })
}

/// Strip a secret out of an error so logs/toasts never echo the API key.
pub fn redact_secret(msg: &str, key: &str) -> String {
    let key = key.trim();
    if key.is_empty() || key.len() < 4 {
        return msg.to_string();
    }
    msg.replace(key, "***")
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

    #[test]
    fn openai_url_appends_chat_completions() {
        assert_eq!(openai_chat_url(""), OPENAI_CHAT_DEFAULT);
        assert_eq!(
            openai_chat_url("https://api.deepseek.com"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            openai_chat_url("https://api.deepseek.com/v1/"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            openai_chat_url("http://127.0.0.1:11434/v1/chat/completions"),
            "http://127.0.0.1:11434/v1/chat/completions"
        );
        assert_eq!(
            openai_chat_url(QWEN_URL_DEFAULT),
            "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
        );
    }

    #[test]
    fn apply_service_defaults_fills_presets() {
        let mut s = crate::config::TranslateSettings {
            provider: "deepseek".into(),
            api_key: "sk-keep".into(),
            ..crate::config::TranslateSettings::default()
        };
        apply_service_defaults(&mut s);
        assert_eq!(s.provider, "deepseek");
        assert_eq!(s.api_url, DEEPSEEK_URL_DEFAULT);
        assert_eq!(s.model, DEEPSEEK_MODEL_DEFAULT);
        assert_eq!(s.api_key, "sk-keep");
        s.provider = "qwen".into();
        apply_service_defaults(&mut s);
        assert_eq!(s.api_url, QWEN_URL_DEFAULT);
        assert_eq!(s.model, QWEN_MODEL_DEFAULT);
        s.provider = "ollama".into();
        apply_service_defaults(&mut s);
        assert_eq!(s.api_url, OLLAMA_URL_DEFAULT);
        assert_eq!(resolved_openai_url(&s), "http://127.0.0.1:11434/v1/chat/completions");
        let empty = crate::config::TranslateSettings::default();
        assert_eq!(empty.provider, "openai");
        assert_eq!(
            resolved_openai_url(&empty),
            OPENAI_CHAT_DEFAULT
        );
    }

    #[test]
    fn openai_reads_chat_completions_content() {
        let body = r#"{"id":"x","choices":[{"message":{"role":"assistant","content":"Hello"}}]}"#;
        assert_eq!(openai_read_content(body).unwrap(), "Hello");
        let err = r#"{"error":{"message":"Incorrect API key provided"}}"#;
        assert!(openai_read_content(err)
            .unwrap_err()
            .contains("Incorrect API key"));
    }

    #[test]
    fn openai_body_contains_model_not_empty_text() {
        let raw = openai_request_body("deepseek-chat", "並且在2010年", "en").unwrap();
        assert!(raw.contains("deepseek-chat"));
        assert!(raw.contains("並且在2010年"));
        assert!(openai_request_body("m", "  ", "en").is_err());
    }

    #[test]
    fn redact_replaces_key() {
        assert_eq!(
            redact_secret("Bearer sk-secret-abc failed", "sk-secret-abc"),
            "Bearer *** failed"
        );
    }

    #[test]
    fn deepl_parses_text() {
        let body = r#"{"translations":[{"detected_source_language":"ZH","text":"Hello"}]}"#;
        let t = deepl_read_text(body).unwrap();
        assert_eq!(t.text, "Hello");
        assert_eq!(t.detected.as_deref(), Some("ZH"));
        assert_eq!(deepl_target("zh-Hans"), "ZH");
        assert_eq!(deepl_target("en"), "EN");
    }
}
