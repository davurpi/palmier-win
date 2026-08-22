use crate::model::TrackKind;
use crate::{ffmpeg, AppState};
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

pub const MCP_PORT: u16 = 19790;

pub async fn serve(app: AppHandle) {
    let router = Router::new()
        .route("/mcp", post(handle))
        .with_state(app.clone());
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], MCP_PORT));
    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => {
            let _ = axum::serve(listener, router).await;
        }
        Err(e) => eprintln!("MCP server could not bind port {MCP_PORT}: {e}"),
    }
}

async fn handle(State(app): State<AppHandle>, Json(req): Json<Value>) -> impl IntoResponse {
    let method = req["method"].as_str().unwrap_or("").to_string();
    let id = req["id"].clone();

    // Notifications carry no id and expect no body.
    if id.is_null() {
        return (StatusCode::ACCEPTED, Json(json!({}))).into_response();
    }

    let result: Result<Value, String> = match method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2025-06-18",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "palmier-win", "version": env!("CARGO_PKG_VERSION") }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => {
            let name = req["params"]["name"].as_str().unwrap_or("").to_string();
            let args = req["params"]["arguments"].clone();
            match call_tool(&app, &name, args).await {
                Ok(v) => Ok(json!({
                    "content": [{ "type": "text", "text": v.to_string() }],
                    "isError": false
                })),
                Err(e) => Ok(json!({
                    "content": [{ "type": "text", "text": e }],
                    "isError": true
                })),
            }
        }
        _ => Err(format!("method not found: {method}")),
    };

    let body = match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": e } }),
    };
    (StatusCode::OK, Json(body)).into_response()
}

fn tool_definitions() -> Value {
    let str_prop = |d: &str| json!({ "type": "string", "description": d });
    let int_prop = |d: &str| json!({ "type": "integer", "description": d });
    let num_prop = |d: &str| json!({ "type": "number", "description": d });
    json!([
        {
            "name": "get_project",
            "description": "Return the full project state: media assets, timeline settings, tracks and clips. Timeline positions are in frames; source positions in seconds.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "import_media",
            "description": "Import media files (video, audio, image) into the project by absolute path. Returns the created assets with their ids.",
            "inputSchema": { "type": "object", "properties": {
                "paths": { "type": "array", "items": { "type": "string" }, "description": "Absolute file paths" }
            }, "required": ["paths"] }
        },
        {
            "name": "add_track",
            "description": "Add a track. Video tracks are added on top; audio tracks at the bottom.",
            "inputSchema": { "type": "object", "properties": {
                "kind": { "type": "string", "enum": ["video", "audio"] }
            }, "required": ["kind"] }
        },
        {
            "name": "add_clip",
            "description": "Place a media asset on a track. duration_frames defaults to the media's natural length; source_in (seconds) defaults to 0.",
            "inputSchema": { "type": "object", "properties": {
                "track_id": str_prop("Target track id"),
                "media_id": str_prop("Media asset id"),
                "start_frame": int_prop("Timeline start position in frames"),
                "duration_frames": int_prop("Optional clip length in frames"),
                "source_in": num_prop("Optional offset into the source, seconds")
            }, "required": ["track_id", "media_id", "start_frame"] }
        },
        {
            "name": "move_clip",
            "description": "Move a clip to a new start frame, optionally onto another track.",
            "inputSchema": { "type": "object", "properties": {
                "clip_id": str_prop("Clip id"),
                "start_frame": int_prop("New start frame"),
                "track_id": str_prop("Optional target track id")
            }, "required": ["clip_id", "start_frame"] }
        },
        {
            "name": "trim_clip",
            "description": "Trim a clip's in/out point. Trimming the head keeps content in place by advancing source_in.",
            "inputSchema": { "type": "object", "properties": {
                "clip_id": str_prop("Clip id"),
                "start_frame": int_prop("Optional new start frame"),
                "end_frame": int_prop("Optional new end frame")
            }, "required": ["clip_id"] }
        },
        {
            "name": "split_clip",
            "description": "Split a clip at a timeline frame, producing two clips.",
            "inputSchema": { "type": "object", "properties": {
                "clip_id": str_prop("Clip id"),
                "frame": int_prop("Timeline frame to split at")
            }, "required": ["clip_id", "frame"] }
        },
        {
            "name": "remove_clips",
            "description": "Remove clips by id.",
            "inputSchema": { "type": "object", "properties": {
                "clip_ids": { "type": "array", "items": { "type": "string" } }
            }, "required": ["clip_ids"] }
        },
        {
            "name": "set_clip_properties",
            "description": "Set a clip's volume (0-4) and/or opacity (0-1).",
            "inputSchema": { "type": "object", "properties": {
                "clip_id": str_prop("Clip id"),
                "volume": num_prop("Audio volume multiplier"),
                "opacity": num_prop("Video opacity")
            }, "required": ["clip_id"] }
        },
        {
            "name": "set_timeline_settings",
            "description": "Set timeline resolution and/or fps.",
            "inputSchema": { "type": "object", "properties": {
                "width": int_prop("Output width"),
                "height": int_prop("Output height"),
                "fps": num_prop("Frames per second")
            } }
        },
        {
            "name": "export_project",
            "description": "Render the timeline to an MP4 file with FFmpeg. Runs in background; progress is shown in the app.",
            "inputSchema": { "type": "object", "properties": {
                "output_path": str_prop("Absolute path of the output .mp4"),
                "codec": { "type": "string", "enum": ["h264", "h265"], "description": "Defaults to h264" }
            }, "required": ["output_path"] }
        }
    ])
}

fn notify_changed(app: &AppHandle) {
    let state = app.state::<AppState>();
    let project = state.project.lock().unwrap().clone();
    let _ = app.emit("project-changed", project);
}

async fn call_tool(app: &AppHandle, name: &str, args: Value) -> Result<Value, String> {
    let state = app.state::<AppState>();
    let res: Result<Value, String> = match name {
        "get_project" => {
            let p = state.project.lock().unwrap().clone();
            return serde_json::to_value(p).map_err(|e| e.to_string());
        }
        "import_media" => {
            let paths: Vec<String> = args["paths"]
                .as_array()
                .ok_or("paths must be an array")?
                .iter()
                .filter_map(|p| p.as_str().map(String::from))
                .collect();
            let mut imported = vec![];
            for path in paths {
                let mut asset = ffmpeg::probe(&path).await.map_err(|e| e.to_string())?;
                if let Ok(t) = ffmpeg::thumbnail(&asset, &state.cache_dir).await {
                    asset.thumbnail = Some(t.to_string_lossy().to_string());
                }
                state.project.lock().unwrap().media.push(asset.clone());
                imported.push(asset);
            }
            serde_json::to_value(imported).map_err(|e| e.to_string())
        }
        "add_track" => {
            let kind = match args["kind"].as_str() {
                Some("audio") => TrackKind::Audio,
                _ => TrackKind::Video,
            };
            let t = state.project.lock().unwrap().add_track(kind)?;
            serde_json::to_value(t).map_err(|e| e.to_string())
        }
        "add_clip" => {
            let c = state.project.lock().unwrap().add_clip(
                args["track_id"].as_str().ok_or("track_id required")?,
                args["media_id"].as_str().ok_or("media_id required")?,
                args["start_frame"].as_i64().ok_or("start_frame required")?,
                args["duration_frames"].as_i64(),
                args["source_in"].as_f64(),
            )?;
            serde_json::to_value(c).map_err(|e| e.to_string())
        }
        "move_clip" => {
            let c = state.project.lock().unwrap().move_clip(
                args["clip_id"].as_str().ok_or("clip_id required")?,
                args["start_frame"].as_i64().ok_or("start_frame required")?,
                args["track_id"].as_str().map(String::from),
            )?;
            serde_json::to_value(c).map_err(|e| e.to_string())
        }
        "trim_clip" => {
            let c = state.project.lock().unwrap().trim_clip(
                args["clip_id"].as_str().ok_or("clip_id required")?,
                args["start_frame"].as_i64(),
                args["end_frame"].as_i64(),
            )?;
            serde_json::to_value(c).map_err(|e| e.to_string())
        }
        "split_clip" => {
            let c = state.project.lock().unwrap().split_clip(
                args["clip_id"].as_str().ok_or("clip_id required")?,
                args["frame"].as_i64().ok_or("frame required")?,
            )?;
            serde_json::to_value(c).map_err(|e| e.to_string())
        }
        "remove_clips" => {
            let ids: Vec<String> = args["clip_ids"]
                .as_array()
                .ok_or("clip_ids must be an array")?
                .iter()
                .filter_map(|p| p.as_str().map(String::from))
                .collect();
            let n = state.project.lock().unwrap().remove_clips(&ids);
            Ok(json!({ "removed": n }))
        }
        "set_clip_properties" => {
            let c = state.project.lock().unwrap().set_clip_properties(
                args["clip_id"].as_str().ok_or("clip_id required")?,
                args["volume"].as_f64().map(|v| v as f32),
                args["opacity"].as_f64().map(|v| v as f32),
            )?;
            serde_json::to_value(c).map_err(|e| e.to_string())
        }
        "set_timeline_settings" => {
            let mut p = state.project.lock().unwrap();
            if let Some(w) = args["width"].as_u64() {
                p.timeline.width = w as u32;
            }
            if let Some(h) = args["height"].as_u64() {
                p.timeline.height = h as u32;
            }
            if let Some(f) = args["fps"].as_f64() {
                p.timeline.fps = f;
            }
            serde_json::to_value(p.timeline.clone()).map_err(|e| e.to_string())
        }
        "export_project" => {
            let output = args["output_path"].as_str().ok_or("output_path required")?.to_string();
            let codec = args["codec"].as_str().unwrap_or("h264").to_string();
            let project = state.project.lock().unwrap().clone();
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = ffmpeg::export(handle, project, output, codec).await;
            });
            Ok(json!({ "status": "export started" }))
        }
        _ => Err(format!("unknown tool: {name}")),
    };
    if res.is_ok() && name != "get_project" {
        notify_changed(app);
    }
    res
}
