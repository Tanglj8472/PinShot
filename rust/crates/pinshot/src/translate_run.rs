//! Edge `translatetext` request used by the in-place F5 strip.

use pinshot_core::translate::{
    effective_target, lang_family, read_translation_ex, request_body, request_url,
    target_from_ui_lang,
};

pub fn default_target() -> String {
    let lang = pinshot_core::config::load()
        .map(|c| c.language)
        .unwrap_or_else(|_| "zh_CN".into());
    target_from_ui_lang(&lang).to_string()
}

pub fn translate(text: &str, target: &str) -> Result<String, String> {
    let mut to = effective_target(text, target);
    let out = post_translate(text, &to)?;
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
            return Ok(post_translate(text, &to)?.text);
        }
    }
    Ok(out.text)
}

fn post_translate(
    text: &str,
    target: &str,
) -> Result<pinshot_core::translate::Translation, String> {
    let body = request_body(text)?;
    let url = request_url(target);
    crate::logutil::log(&format!(
        "translate {} chars -> {target}",
        text.chars().count()
    ));
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
