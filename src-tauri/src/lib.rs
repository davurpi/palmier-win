mod ffmpeg;
mod mcp;
mod model;

use model::{Clip, MediaAsset, Project, Track, TrackKind};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

pub struct AppState {
    pub project: Mutex<Project>,
    pub cache_dir: PathBuf,
}

type CmdResult<T> = Result<T, String>;

#[tauri::command]
fn get_project(state: tauri::State<AppState>) -> Project {
    state.project.lock().unwrap().clone()
}

#[tauri::command]
async fn import_media(
    state: tauri::State<'_, AppState>,
    paths: Vec<String>,
) -> CmdResult<Vec<MediaAsset>> {
    let mut imported = vec![];
    for path in paths {
        let mut asset = ffmpeg::probe(&path).await.map_err(|e| e.to_string())?;
        if let Ok(t) = ffmpeg::thumbnail(&asset, &state.cache_dir).await {
            asset.thumbnail = Some(t.to_string_lossy().to_string());
        }
        state.project.lock().unwrap().media.push(asset.clone());
        imported.push(asset);
    }
    Ok(imported)
}

#[tauri::command]
fn add_track(state: tauri::State<AppState>, kind: String) -> CmdResult<Track> {
    let kind = if kind == "audio" { TrackKind::Audio } else { TrackKind::Video };
    state.project.lock().unwrap().add_track(kind)
}

#[tauri::command]
fn remove_track(state: tauri::State<AppState>, track_id: String) -> CmdResult<()> {
    state.project.lock().unwrap().remove_track(&track_id)
}

#[tauri::command]
fn add_clip(
    state: tauri::State<AppState>,
    track_id: String,
    media_id: String,
    start_frame: i64,
    duration_frames: Option<i64>,
    source_in: Option<f64>,
) -> CmdResult<Clip> {
    state
        .project
        .lock()
        .unwrap()
        .add_clip(&track_id, &media_id, start_frame, duration_frames, source_in)
}

#[tauri::command]
fn move_clip(
    state: tauri::State<AppState>,
    clip_id: String,
    start_frame: i64,
    track_id: Option<String>,
) -> CmdResult<Clip> {
    state.project.lock().unwrap().move_clip(&clip_id, start_frame, track_id)
}

#[tauri::command]
fn trim_clip(
    state: tauri::State<AppState>,
    clip_id: String,
    start_frame: Option<i64>,
    end_frame: Option<i64>,
) -> CmdResult<Clip> {
    state.project.lock().unwrap().trim_clip(&clip_id, start_frame, end_frame)
}

#[tauri::command]
fn split_clip(state: tauri::State<AppState>, clip_id: String, frame: i64) -> CmdResult<Vec<Clip>> {
    state.project.lock().unwrap().split_clip(&clip_id, frame)
}

#[tauri::command]
fn remove_clips(state: tauri::State<AppState>, clip_ids: Vec<String>) -> usize {
    state.project.lock().unwrap().remove_clips(&clip_ids)
}

#[tauri::command]
fn set_clip_properties(
    state: tauri::State<AppState>,
    clip_id: String,
    volume: Option<f32>,
    opacity: Option<f32>,
) -> CmdResult<Clip> {
    state.project.lock().unwrap().set_clip_properties(&clip_id, volume, opacity)
}

#[tauri::command]
fn set_timeline_settings(
    state: tauri::State<AppState>,
    width: Option<u32>,
    height: Option<u32>,
    fps: Option<f64>,
) -> CmdResult<()> {
    let mut p = state.project.lock().unwrap();
    if let Some(w) = width {
        p.timeline.width = w;
    }
    if let Some(h) = height {
        p.timeline.height = h;
    }
    if let Some(f) = fps {
        p.timeline.fps = f;
    }
    Ok(())
}

#[tauri::command]
fn save_project(state: tauri::State<AppState>, path: String) -> CmdResult<()> {
    let mut p = state.project.lock().unwrap();
    p.path = Some(path.clone());
    let json = serde_json::to_string_pretty(&*p).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn load_project(state: tauri::State<AppState>, path: String) -> CmdResult<Project> {
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut loaded: Project = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    loaded.path = Some(path);
    *state.project.lock().unwrap() = loaded.clone();
    Ok(loaded)
}

#[tauri::command]
fn new_project(state: tauri::State<AppState>) -> Project {
    let fresh = Project::default();
    *state.project.lock().unwrap() = fresh.clone();
    fresh
}

#[tauri::command]
async fn export_project(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    output_path: String,
    codec: String,
) -> CmdResult<()> {
    let project = state.project.lock().unwrap().clone();
    tauri::async_runtime::spawn(async move {
        let _ = ffmpeg::export(app, project, output_path, codec).await;
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let cache_dir = app
                .path()
                .app_cache_dir()
                .unwrap_or_else(|_| std::env::temp_dir())
                .join("thumbnails");
            app.manage(AppState {
                project: Mutex::new(Project::default()),
                cache_dir,
            });
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                mcp::serve(handle).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_project,
            import_media,
            add_track,
            remove_track,
            add_clip,
            move_clip,
            trim_clip,
            split_clip,
            remove_clips,
            set_clip_properties,
            set_timeline_settings,
            save_project,
            load_project,
            new_project,
            export_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
