//! F5 translation: Edge (free default), OpenAI-compatible chat-completions, or DeepL.

use pinshot_core::config::TranslateSettings;
use pinshot_core::translate::{
    deepl_form_body, deepl_read_text, deepl_url, effective_target, lang_family, openai_chat_url,
    openai_read_content, openai_request_body, read_translation_ex, redact_secret, request_body,
    request_url, target_from_ui_lang,
};

pub fn default_target() -> String {
    let cfg = pinshot_core::config::load().unwrap_or_default();
    target_for(&cfg.translate, &cfg.language)
}

pub fn target_for(settings: &TranslateSettings, ui_lang: &str) -> String {
    let t = settings.target.trim();
    if t.is_empty() {
        target_from_ui_lang(ui_lang).to_string()
    } else {
        t.to_string()
    }
}

pub fn translate(text: &str, target: &str) -> Result<String, String> {
    let cfg = pinshot_core::config::load().unwrap_or_default();
    let mut settings = cfg.translate;
    if settings.target.trim().is_empty() {
        settings.target = target.to_string();
    }
    translate_with(&settings, &cfg.language, text)
}

pub fn translate_with(
    settings: &TranslateSettings,
    ui_lang: &str,
    text: &str,
) -> Result<String, String> {
    let configured = target_for(settings, ui_lang);
    let to = effective_target(text, &configured);
    match settings.normalized_provider() {
        "openai" => openai_translate(settings, text, &to),
        "deepl" => deepl_translate(settings, text, &to),
        _ => edge_translate(text, &to),
    }
}

fn log_translate(provider: &str, n: usize, target: &str) {
    crate::logutil::log(&format!("translate {n} chars -> {target} via {provider}"));
}

fn edge_translate(text: &str, target: &str) -> Result<String, String> {
    let mut to = target.to_string();
    let out = post_edge(text, &to)?;
    if let Some(det) = out.detected.as_deref() {
        if lang_family(det) == lang_family(&to) {
            let retry = if lang_family(&to) == "en" {
                "zh-Hans"
            } else {
                "en"
            };
            crate::logutil::log(&format!(
                "translate detected {det} already {to}; retry {retry}"
            ));
            to = retry.into();
            return Ok(post_edge(text, &to)?.text);
        }
    }
    Ok(out.text)
}

fn post_edge(text: &str, target: &str) -> Result<pinshot_core::translate::Translation, String> {
    let body = request_body(text)?;
    let url = request_url(target);
    log_translate("edge", text.chars().count(), target);
    let resp = ureq::post(&url)
        .set("Content-Type", "application/json")
        .set(
            "User-Agent",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/128.0.0.0 Safari/537.36 Edg/128.0.0.0",
        )
        .timeout(std::time::Duration::from_secs(20))
        .send_string(&body)
        .map_err(|e| e.to_string())?;
    let raw = resp.into_string().map_err(|e| e.to_string())?;
    read_translation_ex(&raw)
}

fn openai_translate(
    settings: &TranslateSettings,
    text: &str,
    target: &str,
) -> Result<String, String> {
    let url = openai_chat_url(&settings.api_url);
    let body = openai_request_body(&settings.model, text, target)?;
    log_translate("openai", text.chars().count(), target);
    let mut req = ureq::post(&url)
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(30));
    if !settings.api_key.trim().is_empty() {
        req = req.set(
            "Authorization",
            &format!("Bearer {}", settings.api_key.trim()),
        );
    }
    let raw = match req.send_string(&body) {
        Ok(resp) => resp.into_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(code, resp)) => {
            let err_body = resp.into_string().unwrap_or_default();
            let msg = openai_read_content(&err_body).unwrap_or_else(|_| {
                if code == 401 {
                    "Invalid API key".into()
                } else {
                    format!("HTTP {code}")
                }
            });
            return Err(redact_secret(&msg, &settings.api_key));
        }
        Err(e) => return Err(redact_secret(&e.to_string(), &settings.api_key)),
    };
    openai_read_content(&raw).map_err(|e| redact_secret(&e, &settings.api_key))
}

fn deepl_translate(
    settings: &TranslateSettings,
    text: &str,
    target: &str,
) -> Result<String, String> {
    let url = deepl_url(&settings.api_url);
    let form = deepl_form_body(text, target)?;
    if settings.api_key.trim().is_empty() {
        return Err("DeepL API key required".into());
    }
    log_translate("deepl", text.chars().count(), target);
    let raw = match ureq::post(&url)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .set(
            "Authorization",
            &format!("DeepL-Auth-Key {}", settings.api_key.trim()),
        )
        .timeout(std::time::Duration::from_secs(20))
        .send_string(&form)
    {
        Ok(resp) => resp.into_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(code, resp)) => {
            let err_body = resp.into_string().unwrap_or_default();
            let msg = if code == 403 || code == 401 {
                "Invalid API key".into()
            } else if err_body.is_empty() {
                format!("HTTP {code}")
            } else {
                err_body
            };
            return Err(redact_secret(&msg, &settings.api_key));
        }
        Err(e) => return Err(redact_secret(&e.to_string(), &settings.api_key)),
    };
    Ok(deepl_read_text(&raw)?.text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinshot_core::config::TranslateSettings;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    fn spawn_mock(status: u16, body: &str) -> (String, Arc<Mutex<String>>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let captured = Arc::new(Mutex::new(String::new()));
        let cap = captured.clone();
        let body = body.to_string();
        let h = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).ok();
            let mut buf = Vec::new();
            let mut tmp = [0u8; 1024];
            for _ in 0..32 {
                match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => buf.extend_from_slice(&tmp[..n]),
                    Err(_) => break,
                }
                if let Some(head_end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = &buf[..head_end];
                    let hs = String::from_utf8_lossy(headers);
                    let cl = hs
                        .lines()
                        .find_map(|l| {
                            l.split_once(':').and_then(|(k, v)| {
                                if k.eq_ignore_ascii_case("content-length") {
                                    v.trim().parse::<usize>().ok()
                                } else {
                                    None
                                }
                            })
                        })
                        .unwrap_or(0);
                    if buf.len() >= head_end + 4 + cl {
                        break;
                    }
                }
            }
            *cap.lock().unwrap() = String::from_utf8_lossy(&buf).into_owned();
            let reason = if status == 200 { "OK" } else { "ERR" };
            let resp = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        });
        (format!("http://{addr}/v1/chat/completions"), captured, h)
    }

    #[test]
    fn openai_compatible_mock_translates_and_sends_bearer() {
        let json = r#"{"choices":[{"message":{"role":"assistant","content":"Hello from mock"}}]}"#;
        let (url, seen, h) = spawn_mock(200, json);
        let settings = TranslateSettings {
            provider: "openai".into(),
            api_url: url,
            api_key: "sk-test-secret-key-xyz".into(),
            model: "gpt-4o-mini".into(),
            target: "en".into(),
        };
        let out = translate_with(&settings, "zh_CN", "你好").unwrap();
        assert_eq!(out, "Hello from mock");
        let _ = h.join();
        let req = seen.lock().unwrap().clone();
        assert!(
            req.contains("Authorization: Bearer sk-test-secret-key-xyz"),
            "missing bearer in {req:?}"
        );
        assert!(req.contains("gpt-4o-mini"), "missing model in {req:?}");
        assert!(
            !req.contains("translate sk-test"),
            "request dump should not be our log line"
        );
    }

    #[test]
    fn openai_mock_401_does_not_echo_key() {
        let json = r#"{"error":{"message":"Incorrect API key provided: sk-test-secret-key-xyz"}}"#;
        let (url, _, h) = spawn_mock(401, json);
        let settings = TranslateSettings {
            provider: "openai".into(),
            api_url: url,
            api_key: "sk-test-secret-key-xyz".into(),
            model: "gpt-4o-mini".into(),
            target: "en".into(),
        };
        let err = translate_with(&settings, "en", "hello").unwrap_err();
        let _ = h.join();
        assert!(!err.contains("sk-test-secret-key-xyz"), "leaked key: {err}");
        assert!(err.contains("***") || err.contains("Invalid") || err.contains("Incorrect"));
    }

    #[test]
    fn target_follows_settings_then_ui() {
        let mut s = TranslateSettings::default();
        assert_eq!(target_for(&s, "zh_CN"), "zh-Hans");
        assert_eq!(target_for(&s, "en"), "en");
        s.target = "ja".into();
        assert_eq!(target_for(&s, "zh_CN"), "ja");
    }

    #[test]
    fn deepl_mock_translates_form_post() {
        let json = r#"{"translations":[{"detected_source_language":"ZH","text":"Hello DeepL"}]}"#;
        let (url, seen, h) = spawn_mock(200, json);
        let settings = TranslateSettings {
            provider: "deepl".into(),
            api_url: url,
            api_key: "deepl-secret-xyz".into(),
            model: String::new(),
            target: "en".into(),
        };
        let out = translate_with(&settings, "zh_CN", "你好").unwrap();
        assert_eq!(out, "Hello DeepL");
        let _ = h.join();
        let req = seen.lock().unwrap().clone();
        assert!(
            req.contains("DeepL-Auth-Key deepl-secret-xyz"),
            "missing DeepL auth in {req:?}"
        );
        assert!(req.contains("target_lang=EN"), "missing target in {req:?}");
    }
}
