//! Edge `translatetext` request used by the in-place F5 strip.

use pinshot_core::translate::{read_translation, request_body, request_url, TARGETS};

pub fn default_target() -> &'static str {
    TARGETS.first().map(|t| t.1).unwrap_or("zh-Hans")
}

pub fn translate(text: &str, target: &str) -> Result<String, String> {
    let body = request_body(text)?;
    let url = request_url(target);
    crate::logutil::log(&format!("translate {} chars -> {target}", text.chars().count()));
    let resp = ureq::post(&url)
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(20))
        .send_string(&body)
        .map_err(|e| e.to_string())?;
    let raw = resp.into_string().map_err(|e| e.to_string())?;
    read_translation(&raw)
}
