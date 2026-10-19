//! Opt-in native automation: persistent documents, native capture and rendering.
use std::collections::BTreeMap;
use std::path::Path;
use std::io::Write;

use pinshot_core::annotation::{ArrowKind, DrawMode, Effect, Shape, Style, Tool};
use pinshot_core::color::Rgba;
use pinshot_core::geom::Point;
use pinshot_core::raster::Raster;
use serde::Deserialize;
use serde_json::{json, Value};

struct Document {
    image: Raster,
    origin: (i32, i32),
    backend: String,
    shapes: Vec<(u64, Shape)>,
    shown: bool,
}

#[derive(Default)]
struct Automation {
    documents: BTreeMap<u64, Document>,
    next_id: u64,
    proxy: Option<winit::event_loop::EventLoopProxy<crate::event::AppEvent>>,
}

#[derive(Deserialize)]
struct Annotation {
    kind: String,
    points: Vec<Point>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    color: Option<Rgba>,
    #[serde(default)]
    width: Option<i32>,
    #[serde(default)]
    strength: Option<i32>,
}

impl Annotation {
    fn shape(self, image: &Raster) -> Result<Shape, String> {
        let tool = match self.kind.as_str() {
            "rect" => Tool::Rect,
            "ellipse" => Tool::Ellipse,
            "line" | "arrow" => Tool::Line,
            "text" => Tool::Text,
            "blur" | "mosaic" => Tool::Mosaic,
            _ => return Err("Unsupported annotation kind".into()),
        };
        let count = if tool == Tool::Text { 1 } else { 2 };
        if self.points.len() != count || self.points.iter().any(|p| {
            !p.x.is_finite() || !p.y.is_finite() || p.x < 0.0 || p.y < 0.0
                || p.x >= image.width as f32 || p.y >= image.height as f32
        }) {
            return Err("Points must be inside the image; text needs one point, other shapes two".into());
        }
        if tool == Tool::Text && (self.text.is_empty() || self.text.len() > 16384) {
            return Err("Text must contain 1 to 16384 UTF-8 bytes".into());
        }
        let mut style = Style::for_tool(tool);
        if let Some(color) = self.color { style.color = color; }
        if let Some(width) = self.width {
            if !(1..=200).contains(&width) { return Err("Width must be 1..200".into()); }
            style.width = width;
            style.font_size = width;
        }
        if let Some(strength) = self.strength {
            if !(1..=30).contains(&strength) { return Err("Strength must be 1..30".into()); }
            style.size = strength;
        }
        if self.kind == "arrow" { style.set_arrow_kind(ArrowKind::Thin); }
        if tool == Tool::Mosaic {
            style.mode = DrawMode::Rect;
            style.effect = if self.kind == "blur" { Effect::Blur } else { Effect::Mosaic };
        }
        let mut shape = Shape::new(tool, self.points, style);
        shape.text = self.text;
        Ok(shape)
    }
}

impl Automation {
    fn id(&mut self) -> u64 { self.next_id += 1; self.next_id }

    fn view_request(&self, action: &str, session_id: u64, image: Option<std::sync::Arc<Raster>>) -> Result<Value, String> {
        let proxy = self.proxy.as_ref().ok_or("No GUI event loop is connected")?;
        let (response, result) = std::sync::mpsc::channel();
        proxy.send_event(crate::event::AppEvent::AutomationView {
            action: action.into(), session_id, image, response,
        }).map_err(|e| e.to_string())?;
        result.recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|_| "Preview window did not respond within 10 seconds".to_string())?
    }

    fn refresh_view(&mut self, session: u64, show: bool) -> Result<Value, String> {
        let document = self.documents.get(&session).ok_or("Unknown session_id")?;
        let shapes = document.shapes.iter().map(|(_, shape)| shape.clone()).collect::<Vec<_>>();
        let image = std::sync::Arc::new(pinshot_core::render::compose(&document.image, &shapes));
        let result = self.view_request(if show { "show" } else { "update" }, session, Some(image))?;
        if let Some(document) = self.documents.get_mut(&session) {
            document.shown = result["visible"].as_bool().unwrap_or(false);
        }
        Ok(result)
    }

    fn execute(&mut self, request: &Value) -> Result<Value, String> {
        let op = request["op"].as_str().ok_or("Missing op")?;
        if op == "status" {
            let documents: Vec<_> = self.documents.iter().map(|(id, d)| json!({
                "session_id": id, "width": d.image.width, "height": d.image.height,
                "origin": d.origin, "backend": d.backend,
                "annotations": d.shapes.iter().map(|(id, shape)| json!({"annotation_id": id, "shape": shape})).collect::<Vec<_>>()
            })).collect();
            let views = if self.proxy.is_some() { self.view_request("state", 0, None)? } else { json!([]) };
            return Ok(json!({"protocol": 1, "pid": std::process::id(), "sessions": documents, "views": views}));
        }
        if op == "capture" {
            if self.documents.len() >= 4 { return Err("Close a session first (maximum 4)".into()); }
            let capture = crate::capture::Capture::grab(false)?;
            if u64::from(capture.composed.width) * u64::from(capture.composed.height) > 40_000_000 {
                return Err("Desktop exceeds 40 million pixels".into());
            }
            let id = self.id();
            let result = json!({"session_id": id, "width": capture.composed.width,
                "height": capture.composed.height, "origin": capture.virtual_origin,
                "backend": capture.backend});
            self.documents.insert(id, Document { image: capture.composed,
                origin: capture.virtual_origin, backend: capture.backend, shapes: Vec::new(), shown: false });
            return Ok(result);
        }
        let session = request["session_id"].as_u64().ok_or("Missing session_id")?;
        if !self.documents.contains_key(&session) { return Err("Unknown session_id".into()); }
        if op == "show" { return self.refresh_view(session, true); }
        if op == "hide" {
            let result = self.view_request("hide", session, None)?;
            self.documents.get_mut(&session).ok_or("Unknown session_id")?.shown = false;
            return Ok(result);
        }
        if op == "close" {
            if self.proxy.is_some() { self.view_request("hide", session, None)?; }
            self.documents.remove(&session);
            return Ok(json!({"closed": session}));
        }
        let annotation_id = if op == "annotate" { Some(self.id()) } else { None };
        let document = self.documents.get_mut(&session).ok_or("Unknown session_id")?;
        match op {
            "annotate" => {
                if document.shapes.len() >= 1000 { return Err("Annotation limit reached".into()); }
                let spec: Annotation = serde_json::from_value(request["annotation"].clone()).map_err(|e| e.to_string())?;
                let shape = spec.shape(&document.image)?;
                let id = annotation_id.ok_or("Missing annotation id")?;
                document.shapes.push((id, shape));
                Ok(json!({"session_id": session, "annotation_id": id}))
            }
            "remove" => {
                let id = request["annotation_id"].as_u64().ok_or("Missing annotation_id")?;
                let index = document.shapes.iter().position(|(key, _)| *key == id).ok_or("Unknown annotation_id")?;
                document.shapes.remove(index);
                Ok(json!({"removed": id}))
            }
            "preview" | "save" => {
                let path = Path::new(request["path"].as_str().ok_or("Missing path")?);
                if !path.is_absolute() || !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
                    return Err("An absolute PNG path is required".into());
                }
                if path.exists() { return Err("Output already exists; choose another path".into()); }
                let shapes: Vec<_> = document.shapes.iter().map(|(_, shape)| shape.clone()).collect();
                let image = pinshot_core::render::compose(&document.image, &shapes);
                // Reserve the output so existing files are never silently overwritten.
                let file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| e.to_string())?;
                let rgba = image::RgbaImage::from_raw(image.width, image.height, image.pixels).ok_or("Invalid raster")?;
                let mut writer = std::io::BufWriter::new(file);
                let encoded = image::DynamicImage::ImageRgba8(rgba).write_to(&mut writer, image::ImageFormat::Png)
                    .map_err(|e| e.to_string()).and_then(|_| writer.flush().map_err(|e| e.to_string()));
                drop(writer);
                if let Err(error) = encoded {
                    let _ = std::fs::remove_file(path);
                    return Err(error);
                }
                Ok(json!({"session_id": session, "path": path, "width": image.width, "height": image.height}))
            }
            _ => Err("Unknown operation".into()),
        }
    }

    fn reply(&mut self, request: Value) -> Value {
        match self.execute(&request) {
            Ok(mut result) => {
                let capture_show = request["op"] == "capture" && request["show"] == true;
                let session = if capture_show { result["session_id"].as_u64() } else { request["session_id"].as_u64() };
                if let Some(session) = session {
                    let editing = matches!(request["op"].as_str(), Some("annotate" | "remove"));
                    if capture_show || (editing && self.documents.get(&session).is_some_and(|d| d.shown)) {
                        match self.refresh_view(session, capture_show) {
                            Ok(view) => result["view"] = view,
                            Err(error) => {
                                // The document mutation succeeded; report view errors without encouraging duplicate edits.
                                result["view_error"] = json!(error);
                                result["mutation_applied"] = json!(true);
                            }
                        }
                    }
                }
                json!({"id": request["id"], "result": result})
            }
            Err(error) => json!({"id": request["id"], "error": error}),
        }
    }
}

#[cfg(windows)]
pub fn spawn_server(proxy: winit::event_loop::EventLoopProxy<crate::event::AppEvent>) -> Result<(), String> {
    std::thread::Builder::new().name("pinshot-automation".into()).spawn(move || {
        let mut state = Automation { proxy: Some(proxy), ..Automation::default() };
        loop {
            if let Err(error) = serve_one(&mut state) {
                crate::logutil::error(&format!("automation: {error}"));
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(windows))]
pub fn spawn_server(_proxy: winit::event_loop::EventLoopProxy<crate::event::AppEvent>) -> Result<(), String> {
    Err("Automation transport currently supports Windows only".into())
}

#[cfg(windows)]
fn serve_one(state: &mut Automation) -> Result<(), String> {
    use windows::core::w;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE};
    use windows::Win32::Storage::FileSystem::{FlushFileBuffers, ReadFile, WriteFile, PIPE_ACCESS_DUPLEX};
    use windows::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT, PIPE_REJECT_REMOTE_CLIENTS};
    struct Pipe(HANDLE);
    impl Drop for Pipe { fn drop(&mut self) { unsafe { let _ = DisconnectNamedPipe(self.0); let _ = CloseHandle(self.0); } } }
    unsafe {
        let handle = CreateNamedPipeW(w!(r"\\.\pipe\PinShotAutomation-v1"), PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1, 65536, 65536, 0, None);
        if handle == INVALID_HANDLE_VALUE { return Err("Cannot create automation pipe".into()); }
        let pipe = Pipe(handle);
        if ConnectNamedPipe(pipe.0, None).is_err() && GetLastError() != ERROR_PIPE_CONNECTED { return Err("Automation connect failed".into()); }
        let mut bytes = Vec::new();
        loop {
            let mut buffer = [0u8; 4096];
            let mut count = 0;
            ReadFile(pipe.0, Some(&mut buffer), Some(&mut count), None).map_err(|e| e.to_string())?;
            if count == 0 { return Err("Automation client disconnected".into()); }
            bytes.extend_from_slice(&buffer[..count as usize]);
            if bytes.len() > 65536 { return Err("Automation request too large".into()); }
            if bytes.contains(&b'\n') { break; }
        }
        let reply = match serde_json::from_slice::<Value>(&bytes) {
            Ok(request) => state.reply(request),
            Err(_) => json!({"id": null, "error": "Invalid JSON request"}),
        };
        let payload = format!("{reply}\n");
        let mut remaining = payload.as_bytes();
        while !remaining.is_empty() {
            let mut written = 0;
            WriteFile(pipe.0, Some(remaining), Some(&mut written), None).map_err(|e| e.to_string())?;
            if written == 0 { return Err("Automation write failed".into()); }
            remaining = &remaining[written as usize..];
        }
        let _ = FlushFileBuffers(pipe.0);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_export_renders_and_refuses_overwrite() {
        let directory = std::env::temp_dir().join(format!("pinshot-automation-test-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&directory).unwrap();
        let output = directory.join("result.png");
        let mut state = Automation::default();
        let mut image = Raster::new(64, 64);
        image.force_opaque();
        state.documents.insert(1, Document { image, origin: (0, 0), backend: "test".into(), shapes: Vec::new(), shown: false });
        state.next_id = 1;
        state.execute(&json!({"op":"annotate","session_id":1,"annotation":{"kind":"rect","points":[{"x":5,"y":5},{"x":35,"y":35}],"width":3}})).unwrap();
        state.execute(&json!({"op":"save","session_id":1,"path":output})).unwrap();
        let saved = std::fs::read(&output).unwrap();
        let decoded = image::load_from_memory(&saved).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (64, 64));
        assert!(decoded.pixels().any(|p| p.0[0] > 100 && p.0[1] < 50));
        assert!(state.execute(&json!({"op":"save","session_id":1,"path":output})).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), saved);
        std::fs::remove_file(output).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn document_lifecycle_and_annotation_validation() {
        let mut state = Automation::default();
        state.documents.insert(1, Document { image: Raster::new(100, 100), origin: (-100, 0), backend: "test".into(), shapes: Vec::new(), shown: false });
        state.next_id = 1;
        assert!(state.execute(&json!({"op":"annotate","session_id":1,"annotation":{"kind":"rect","points":[{"x":1,"y":1},{"x":50,"y":50}]}})).is_ok());
        assert_eq!(state.documents[&1].shapes.len(), 1);
        assert!(state.execute(&json!({"op":"annotate","session_id":1,"annotation":{"kind":"text","text":"hello","points":[{"x":200,"y":1}]}})).is_err());
        assert_eq!(state.documents[&1].shapes.len(), 1);
        assert!(state.execute(&json!({"op":"remove","session_id":1,"annotation_id":2})).is_ok());
        assert!(state.execute(&json!({"op":"close","session_id":1})).is_ok());
        assert!(state.documents.is_empty());
        assert!(state.execute(&json!({"op":"preview","session_id":1,"path":"x.png"})).is_err());
    }
}
