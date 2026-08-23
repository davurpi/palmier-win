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
            "serverInfo": { "name": "palmier-win", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "Palmier Win video editor. Timeline positions are FRAMES; source positions are SECONDS. Tracks render top-down (index 0 on top). Edits are undoable (undo tool) - do not ask permission for individual edits. Composition (PIP, split screen, grid) is apply_layout's job. Use remove_silence first when tightening pacing. Every change shows live in the app window."
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
            "description": "Set clip properties. All optional: volume (0-4), opacity (0-1), pos_x/pos_y (normalized canvas center, 0.5 = centered), scale (1 = fit canvas), speed (0.25-4, timeline length fixed), fade_in/fade_out (seconds, video+audio), denoise (bool, audio), brightness (-1..1), contrast (0-2), saturation (0-3).",
            "inputSchema": { "type": "object", "properties": {
                "clip_id": str_prop("Clip id"),
                "volume": num_prop("Audio volume multiplier"),
                "opacity": num_prop("Video opacity"),
                "pos_x": num_prop("Center X, 0..1"),
                "pos_y": num_prop("Center Y, 0..1"),
                "scale": num_prop("Uniform scale, 1 = fit canvas"),
                "speed": num_prop("Playback rate 0.25-4"),
                "fade_in": num_prop("Fade in, seconds"),
                "fade_out": num_prop("Fade out, seconds"),
                "denoise": { "type": "boolean", "description": "Denoise audio (afftdn)" },
                "brightness": num_prop("-1..1, 0 neutral"),
                "contrast": num_prop("0..2, 1 neutral"),
                "saturation": num_prop("0..3, 1 neutral")
            }, "required": ["clip_id"] }
        },
        {
            "name": "apply_layout",
            "description": "Compose clips on the canvas: assigns position+scale slots in order. Layouts: fullscreen, pip-top-left, pip-top-right, pip-bottom-left, pip-bottom-right, side-by-side, thirds, grid-2x2. Never build layouts by hand — use this.",
            "inputSchema": { "type": "object", "properties": {
                "clip_ids": { "type": "array", "items": { "type": "string" }, "description": "Clips in slot order (first = main/background slot)" },
                "layout": { "type": "string", "enum": ["fullscreen", "pip-top-left", "pip-top-right", "pip-bottom-left", "pip-bottom-right", "side-by-side", "thirds", "grid-2x2"] }
            }, "required": ["clip_ids", "layout"] }
        },
        {
            "name": "add_text",
            "description": "Add a text overlay clip (rendered with a system font, outlined for legibility). Defaults: topmost video track, 4s, 72px, white, centered. pos_y 0.8 gives a lower-third.",
            "inputSchema": { "type": "object", "properties": {
                "text": str_prop("Text content; \\n for multiple lines"),
                "start_frame": int_prop("Timeline start position in frames"),
                "duration_frames": int_prop("Optional length in frames (default 4s)"),
                "track_id": str_prop("Optional target video track"),
                "font_size": num_prop("Font size in px at canvas resolution (default 72)"),
                "color": str_prop("Hex color like #ffdd00 (default #ffffff)"),
                "pos_x": num_prop("Center X 0..1 (default 0.5)"),
                "pos_y": num_prop("Center Y 0..1 (default 0.5)")
            }, "required": ["text", "start_frame"] }
        },
        {
            "name": "ripple_delete_ranges",
            "description": "Delete timeline frame ranges across ALL tracks and close the gaps. Ranges are [start,end) frame pairs; they are applied back-to-front automatically.",
            "inputSchema": { "type": "object", "properties": {
                "ranges": { "type": "array", "items": { "type": "array", "items": { "type": "integer" }, "minItems": 2, "maxItems": 2 }, "description": "List of [start_frame, end_frame]" }
            }, "required": ["ranges"] }
        },
        {
            "name": "remove_silence",
            "description": "Detect silences in a clip's audio (FFmpeg silencedetect) and ripple-delete them from the timeline. Great first pass to tighten pacing.",
            "inputSchema": { "type": "object", "properties": {
                "clip_id": str_prop("Clip whose audio to analyze"),
                "noise_db": num_prop("Silence threshold in dB (default -35)"),
                "min_duration": num_prop("Minimum silence length in seconds (default 0.4)")
            }, "required": ["clip_id"] }
        },
        {
            "name": "undo",
            "description": "Undo the last edit.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "redo",
            "description": "Redo the last undone edit.",
            "inputSchema": { "type": "object", "properties": {} }
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
            state.snapshot();
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
            state.snapshot();
            let kind = match args["kind"].as_str() {
                Some("audio") => TrackKind::Audio,
                _ => TrackKind::Video,
            };
            let t = state.project.lock().unwrap().add_track(kind)?;
            serde_json::to_value(t).map_err(|e| e.to_string())
        }
        "add_clip" => {
            state.snapshot();
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
            state.snapshot();
            let c = state.project.lock().unwrap().move_clip(
                args["clip_id"].as_str().ok_or("clip_id required")?,
                args["start_frame"].as_i64().ok_or("start_frame required")?,
                args["track_id"].as_str().map(String::from),
            )?;
            serde_json::to_value(c).map_err(|e| e.to_string())
        }
        "trim_clip" => {
            state.snapshot();
            let c = state.project.lock().unwrap().trim_clip(
                args["clip_id"].as_str().ok_or("clip_id required")?,
                args["start_frame"].as_i64(),
                args["end_frame"].as_i64(),
            )?;
            serde_json::to_value(c).map_err(|e| e.to_string())
        }
        "split_clip" => {
            state.snapshot();
            let c = state.project.lock().unwrap().split_clip(
                args["clip_id"].as_str().ok_or("clip_id required")?,
                args["frame"].as_i64().ok_or("frame required")?,
            )?;
            serde_json::to_value(c).map_err(|e| e.to_string())
        }
        "remove_clips" => {
            state.snapshot();
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
            let clip_id = args["clip_id"].as_str().ok_or("clip_id required")?.to_string();
            let props: crate::model::ClipProps =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            state.snapshot();
            let c = state.project.lock().unwrap().set_clip_properties(&clip_id, props)?;
            serde_json::to_value(c).map_err(|e| e.to_string())
        }
        "apply_layout" => {
            let ids: Vec<String> = args["clip_ids"]
                .as_array()
                .ok_or("clip_ids must be an array")?
                .iter()
                .filter_map(|p| p.as_str().map(String::from))
                .collect();
            let layout = args["layout"].as_str().ok_or("layout required")?;
            state.snapshot();
            let c = state.project.lock().unwrap().apply_layout(&ids, layout)?;
            serde_json::to_value(c).map_err(|e| e.to_string())
        }
        "add_text" => {
            let c = state.add_text_clip(
                args["track_id"].as_str().map(String::from),
                args["text"].as_str().ok_or("text required")?,
                args["start_frame"].as_i64().ok_or("start_frame required")?,
                args["duration_frames"].as_i64(),
                args["font_size"].as_f64().map(|v| v as f32),
                args["color"].as_str().map(String::from),
                args["pos_x"].as_f64().map(|v| v as f32),
                args["pos_y"].as_f64().map(|v| v as f32),
            )?;
            serde_json::to_value(c).map_err(|e| e.to_string())
        }
        "ripple_delete_ranges" => {
            let mut ranges: Vec<(i64, i64)> = args["ranges"]
                .as_array()
                .ok_or("ranges must be an array")?
                .iter()
                .filter_map(|r| {
                    let a = r.as_array()?;
                    Some((a.first()?.as_i64()?, a.get(1)?.as_i64()?))
                })
                .collect();
            ranges.sort();
            state.snapshot();
            let mut p = state.project.lock().unwrap();
            let n = ranges.len();
            for (s, e) in ranges.into_iter().rev() {
                p.ripple_delete_range(s, e)?;
            }
            Ok(serde_json::json!({ "deleted_ranges": n }))
        }
        "remove_silence" => {
            state
                .remove_silence_impl(
                    args["clip_id"].as_str().ok_or("clip_id required")?,
                    args["noise_db"].as_f64(),
                    args["min_duration"].as_f64(),
                )
                .await
        }
        "undo" => {
            let p = state.undo_once()?;
            Ok(serde_json::json!({ "ok": true, "duration_frames": p.duration_frames() }))
        }
        "redo" => {
            let p = state.redo_once()?;
            Ok(serde_json::json!({ "ok": true, "duration_frames": p.duration_frames() }))
        }
        "set_timeline_settings" => {
            state.snapshot();
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
