mod ffmpeg;
mod mcp;
mod model;
mod text;

use model::{Clip, ClipProps, MediaAsset, MediaKind, Project, Track, TrackKind};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

pub struct AppState {
    pub project: Mutex<Project>,
    pub undo: Mutex<Vec<Project>>,
    pub redo: Mutex<Vec<Project>>,
    pub cache_dir: PathBuf,
}

impl AppState {
    /// Call before any mutation: pushes the current state for undo.
    pub fn snapshot(&self) {
        let p = self.project.lock().unwrap().clone();
        let mut undo = self.undo.lock().unwrap();
        undo.push(p);
        if undo.len() > 100 {
            undo.remove(0);
        }
        self.redo.lock().unwrap().clear();
    }

    pub fn undo_once(&self) -> Result<Project, String> {
        let mut undo = self.undo.lock().unwrap();
        let prev = undo.pop().ok_or("nothing to undo")?;
        let mut cur = self.project.lock().unwrap();
        self.redo.lock().unwrap().push(cur.clone());
        *cur = prev.clone();
        Ok(prev)
    }

    pub fn redo_once(&self) -> Result<Project, String> {
        let mut redo = self.redo.lock().unwrap();
        let next = redo.pop().ok_or("nothing to redo")?;
        let mut cur = self.project.lock().unwrap();
        self.undo.lock().unwrap().push(cur.clone());
        *cur = next.clone();
        Ok(next)
    }

    /// Shared by the add_text command and MCP tool.
    pub fn add_text_clip(
        &self,
        track_id: Option<String>,
        text: &str,
        start_frame: i64,
        duration_frames: Option<i64>,
        font_size: Option<f32>,
        color: Option<String>,
        pos_x: Option<f32>,
        pos_y: Option<f32>,
    ) -> Result<Clip, String> {
        let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let size = font_size.unwrap_or(72.0);
        let color = color.unwrap_or_else(|| "#ffffff".into());
        let (png, tw, th) =
            text::render_text_png(&self.cache_dir, &id, text, size, &color).map_err(|e| e.to_string())?;
        self.snapshot();
        let mut p = self.project.lock().unwrap();
        let (cw, ch) = (p.timeline.width as f32, p.timeline.height as f32);
        let first_line = text.lines().next().unwrap_or("Text");
        let asset = MediaAsset {
            id: id.clone(),
            path: png.to_string_lossy().to_string(),
            name: format!("Text: {first_line}"),
            kind: MediaKind::Image,
            duration: 0.0,
            width: tw,
            height: th,
            fps: 0.0,
            has_audio: false,
            thumbnail: Some(png.to_string_lossy().to_string()),
        };
        p.media.push(asset);
        let tid = match track_id {
            Some(t) => t,
            None => p
                .timeline
                .tracks
                .iter()
                .find(|t| t.kind == TrackKind::Video)
                .map(|t| t.id.clone())
                .ok_or("no video track")?,
        };
        let fps = p.timeline.fps;
        let dur = duration_frames.unwrap_or((4.0 * fps) as i64);
        let mut clip = p.add_clip(&tid, &id, start_frame, Some(dur), None)?;
        // Natural pixel size on the canvas instead of scale-to-fit.
        let natural = (tw as f32 / cw).max(th as f32 / ch).min(1.0);
        clip = p
            .set_clip_properties(
                &clip.id,
                ClipProps {
                    scale: Some(natural),
                    pos_x,
                    pos_y,
                    ..Default::default()
                },
            )
            .map_err(|e| e.to_string())?;
        Ok(clip)
    }

    /// Detect silences in a clip's source audio and ripple-delete them.
    pub async fn remove_silence_impl(
        &self,
        clip_id: &str,
        noise_db: Option<f64>,
        min_duration: Option<f64>,
    ) -> Result<serde_json::Value, String> {
        let (clip, path, fps) = {
            let p = self.project.lock().unwrap();
            let clip = p
                .timeline
                .tracks
                .iter()
                .flat_map(|t| t.clips.iter())
                .find(|c| c.id == clip_id)
                .cloned()
                .ok_or_else(|| format!("clip {clip_id} not found"))?;
            let media = p
                .media_by_id(&clip.media_id)
                .ok_or("media not found")?
                .clone();
            if !media.has_audio {
                return Err("clip's media has no audio".into());
            }
            (clip, media.path, p.timeline.fps)
        };
        let dur_tl = (clip.end_frame - clip.start_frame) as f64 / fps;
        let src_dur = dur_tl * clip.speed as f64;
        let ranges = ffmpeg::detect_silence(
            &path,
            clip.source_in,
            src_dur,
            noise_db.unwrap_or(-35.0),
            min_duration.unwrap_or(0.4),
        )
        .await
        .map_err(|e| e.to_string())?;
        if ranges.is_empty() {
            return Ok(serde_json::json!({ "removed_ranges": 0 }));
        }
        self.snapshot();
        let mut p = self.project.lock().unwrap();
        let mut frame_ranges: Vec<(i64, i64)> = ranges
            .iter()
            .map(|(s, e)| {
                let fs = clip.start_frame
                    + ((s - clip.source_in) / clip.speed as f64 * fps).round() as i64;
                let fe = clip.start_frame
                    + ((e - clip.source_in) / clip.speed as f64 * fps).round() as i64;
                (fs.max(clip.start_frame), fe.min(clip.end_frame))
            })
            .filter(|(s, e)| e - s > 1)
            .collect();
        // Delete back-to-front so earlier ranges keep their frame positions.
        frame_ranges.sort();
        let n = frame_ranges.len();
        for (s, e) in frame_ranges.into_iter().rev() {
            p.ripple_delete_range(s, e)?;
        }
        Ok(serde_json::json!({ "removed_ranges": n }))
    }
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
    state.snapshot();
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
    state.snapshot();
    state.project.lock().unwrap().add_track(kind)
}

#[tauri::command]
fn remove_track(state: tauri::State<AppState>, track_id: String) -> CmdResult<()> {
    state.snapshot();
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
    state.snapshot();
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
    state.snapshot();
    state.project.lock().unwrap().move_clip(&clip_id, start_frame, track_id)
}

#[tauri::command]
fn trim_clip(
    state: tauri::State<AppState>,
    clip_id: String,
    start_frame: Option<i64>,
    end_frame: Option<i64>,
) -> CmdResult<Clip> {
    state.snapshot();
    state.project.lock().unwrap().trim_clip(&clip_id, start_frame, end_frame)
}

#[tauri::command]
fn split_clip(state: tauri::State<AppState>, clip_id: String, frame: i64) -> CmdResult<Vec<Clip>> {
    state.snapshot();
    state.project.lock().unwrap().split_clip(&clip_id, frame)
}

#[tauri::command]
fn remove_clips(state: tauri::State<AppState>, clip_ids: Vec<String>) -> usize {
    state.snapshot();
    state.project.lock().unwrap().remove_clips(&clip_ids)
}

#[tauri::command]
fn set_clip_properties(
    state: tauri::State<AppState>,
    clip_id: String,
    props: ClipProps,
) -> CmdResult<Clip> {
    state.snapshot();
    state.project.lock().unwrap().set_clip_properties(&clip_id, props)
}

#[tauri::command]
fn apply_layout(
    state: tauri::State<AppState>,
    clip_ids: Vec<String>,
    layout: String,
) -> CmdResult<Vec<Clip>> {
    state.snapshot();
    state.project.lock().unwrap().apply_layout(&clip_ids, &layout)
}

#[tauri::command]
fn ripple_delete_range(
    state: tauri::State<AppState>,
    start_frame: i64,
    end_frame: i64,
) -> CmdResult<()> {
    state.snapshot();
    state.project.lock().unwrap().ripple_delete_range(start_frame, end_frame)
}

#[tauri::command]
fn add_text(
    state: tauri::State<AppState>,
    track_id: Option<String>,
    text: String,
    start_frame: i64,
    duration_frames: Option<i64>,
    font_size: Option<f32>,
    color: Option<String>,
    pos_x: Option<f32>,
    pos_y: Option<f32>,
) -> CmdResult<Clip> {
    state.add_text_clip(track_id, &text, start_frame, duration_frames, font_size, color, pos_x, pos_y)
}

#[tauri::command]
async fn remove_silence(
    state: tauri::State<'_, AppState>,
    clip_id: String,
    noise_db: Option<f64>,
    min_duration: Option<f64>,
) -> CmdResult<serde_json::Value> {
    state.remove_silence_impl(&clip_id, noise_db, min_duration).await
}

#[tauri::command]
fn undo(state: tauri::State<AppState>) -> CmdResult<Project> {
    state.undo_once()
}

#[tauri::command]
fn redo(state: tauri::State<AppState>) -> CmdResult<Project> {
    state.redo_once()
}

#[tauri::command]
fn set_timeline_settings(
    state: tauri::State<AppState>,
    width: Option<u32>,
    height: Option<u32>,
    fps: Option<f64>,
) -> CmdResult<()> {
    state.snapshot();
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
    state.snapshot();
    *state.project.lock().unwrap() = loaded.clone();
    Ok(loaded)
}

#[tauri::command]
fn new_project(state: tauri::State<AppState>) -> Project {
    state.snapshot();
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
                undo: Mutex::new(vec![]),
                redo: Mutex::new(vec![]),
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
            apply_layout,
            ripple_delete_range,
            add_text,
            remove_silence,
            undo,
            redo,
            set_timeline_settings,
            save_project,
            load_project,
            new_project,
            export_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
