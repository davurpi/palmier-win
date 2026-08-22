use crate::model::{MediaAsset, MediaKind, Project, TrackKind};
use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"];

pub async fn probe(path: &str) -> Result<MediaAsset> {
    let out = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-print_format", "json",
            "-show_format", "-show_streams",
            path,
        ])
        .output()
        .await
        .context("ffprobe not found — install FFmpeg and make sure it is on PATH")?;
    if !out.status.success() {
        return Err(anyhow!(
            "ffprobe failed for {path}: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    let video = streams.iter().find(|s| s["codec_type"] == "video");
    let audio = streams.iter().find(|s| s["codec_type"] == "audio");
    let duration: f64 = v["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse().ok())
        .unwrap_or(0.0);

    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let kind = if IMAGE_EXTS.contains(&ext.as_str()) {
        MediaKind::Image
    } else if video.is_some() {
        MediaKind::Video
    } else if audio.is_some() {
        MediaKind::Audio
    } else {
        return Err(anyhow!("{path}: no audio or video streams"));
    };

    let (width, height, fps) = if let Some(vs) = video {
        let w = vs["width"].as_u64().unwrap_or(0) as u32;
        let h = vs["height"].as_u64().unwrap_or(0) as u32;
        let fps = vs["avg_frame_rate"]
            .as_str()
            .and_then(|r| {
                let mut it = r.split('/');
                let num: f64 = it.next()?.parse().ok()?;
                let den: f64 = it.next()?.parse().ok()?;
                (den > 0.0).then(|| num / den)
            })
            .unwrap_or(0.0);
        (w, h, fps)
    } else {
        (0, 0, 0.0)
    };

    Ok(MediaAsset {
        id: Uuid::new_v4().to_string()[..8].to_string(),
        path: path.to_string(),
        name: Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string()),
        kind,
        duration: if kind == MediaKind::Image { 0.0 } else { duration },
        width,
        height,
        fps,
        has_audio: audio.is_some(),
        thumbnail: None,
    })
}

pub async fn thumbnail(asset: &MediaAsset, cache_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(cache_dir)?;
    let out_path = cache_dir.join(format!("{}.png", asset.id));
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y").arg("-v").arg("error");
    if asset.kind == MediaKind::Video && asset.duration > 1.0 {
        cmd.args(["-ss", &format!("{}", (asset.duration * 0.1).min(3.0))]);
    }
    cmd.args(["-i", &asset.path]);
    match asset.kind {
        MediaKind::Audio => {
            cmd.args(["-filter_complex", "showwavespic=s=320x180:colors=#7c6df2", "-frames:v", "1"]);
        }
        _ => {
            cmd.args(["-frames:v", "1", "-vf", "scale=320:-2"]);
        }
    }
    cmd.arg(out_path.to_string_lossy().to_string());
    let out = cmd.output().await?;
    if !out.status.success() {
        return Err(anyhow!(
            "thumbnail failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(out_path)
}

#[derive(Serialize, Clone)]
pub struct ExportProgress {
    pub percent: f64,
    pub done: bool,
    pub error: Option<String>,
}

/// Build and run the FFmpeg command that renders the whole timeline.
///
/// Every clip becomes its own input (with -ss/-t so FFmpeg only decodes what is
/// used). Video tracks are composited bottom-to-top with overlay; audio is mixed
/// with amix. This mirrors what the timeline preview shows.
pub async fn export(
    app: tauri::AppHandle,
    project: Project,
    output: String,
    codec: String,
) -> Result<()> {
    let tl = &project.timeline;
    let fps = tl.fps;
    let (w, h) = (tl.width, tl.height);
    let total_frames = project.duration_frames();
    if total_frames == 0 {
        return Err(anyhow!("timeline is empty"));
    }
    let total_sec = total_frames as f64 / fps;

    let mut args: Vec<String> = vec!["-y".into(), "-v".into(), "error".into()];
    let mut filter = String::new();
    let mut input_idx = 0usize;

    // (input label registration happens as we walk the clips)
    struct VClip {
        label: String,
        start: f64,
        end: f64,
        opacity: f32,
    }
    let mut video_layers: Vec<VClip> = vec![]; // ordered bottom-to-top
    let mut audio_labels: Vec<String> = vec![];

    // Iterate video tracks bottom-to-top so the top track (index 0) is overlaid last.
    for track in tl.tracks.iter().rev() {
        for clip in &track.clips {
            let media = match project.media_by_id(&clip.media_id) {
                Some(m) => m.clone(),
                None => continue,
            };
            let start = clip.start_frame as f64 / fps;
            let end = clip.end_frame as f64 / fps;
            let dur = end - start;

            match (track.kind, media.kind) {
                (TrackKind::Video, MediaKind::Image) => {
                    args.extend(["-loop".into(), "1".into(), "-t".into(), format!("{dur:.4}")]);
                    args.extend(["-i".into(), media.path.clone()]);
                    let lbl = format!("v{input_idx}");
                    filter.push_str(&format!(
                        "[{input_idx}:v]scale={w}:{h}:force_original_aspect_ratio=decrease,\
                         pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={fps},\
                         format=yuva420p,setpts=PTS-STARTPTS+{start:.4}/TB[{lbl}];"
                    ));
                    video_layers.push(VClip { label: lbl, start, end, opacity: clip.opacity });
                    input_idx += 1;
                }
                (TrackKind::Video, MediaKind::Video) => {
                    args.extend([
                        "-ss".into(), format!("{:.4}", clip.source_in),
                        "-t".into(), format!("{dur:.4}"),
                        "-i".into(), media.path.clone(),
                    ]);
                    let lbl = format!("v{input_idx}");
                    filter.push_str(&format!(
                        "[{input_idx}:v]scale={w}:{h}:force_original_aspect_ratio=decrease,\
                         pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={fps},\
                         format=yuva420p,setpts=PTS-STARTPTS+{start:.4}/TB[{lbl}];"
                    ));
                    video_layers.push(VClip { label: lbl, start, end, opacity: clip.opacity });
                    if media.has_audio && clip.volume > 0.0 {
                        let albl = format!("a{input_idx}");
                        filter.push_str(&format!(
                            "[{input_idx}:a]atrim=0:{dur:.4},asetpts=PTS-STARTPTS,\
                             volume={vol},adelay={delay}:all=1[{albl}];",
                            vol = clip.volume,
                            delay = (start * 1000.0).round() as i64,
                        ));
                        audio_labels.push(albl);
                    }
                    input_idx += 1;
                }
                (TrackKind::Audio, _) => {
                    args.extend([
                        "-ss".into(), format!("{:.4}", clip.source_in),
                        "-t".into(), format!("{dur:.4}"),
                        "-i".into(), media.path.clone(),
                    ]);
                    let albl = format!("a{input_idx}");
                    filter.push_str(&format!(
                        "[{input_idx}:a]atrim=0:{dur:.4},asetpts=PTS-STARTPTS,\
                         volume={vol},adelay={delay}:all=1[{albl}];",
                        vol = clip.volume,
                        delay = (start * 1000.0).round() as i64,
                    ));
                    audio_labels.push(albl);
                    input_idx += 1;
                }
                _ => {}
            }
        }
    }

    // Black base canvas, then overlay every video layer in order.
    filter.push_str(&format!(
        "color=black:size={w}x{h}:rate={fps}:d={total_sec:.4}[base];"
    ));
    let mut prev = "base".to_string();
    for (i, l) in video_layers.iter().enumerate() {
        let src = if l.opacity < 0.999 {
            let faded = format!("{}f", l.label);
            filter.push_str(&format!(
                "[{}]colorchannelmixer=aa={}[{faded}];",
                l.label, l.opacity
            ));
            faded
        } else {
            l.label.clone()
        };
        let next = format!("ov{i}");
        filter.push_str(&format!(
            "[{prev}][{src}]overlay=enable='between(t,{:.4},{:.4})':eof_action=pass[{next}];",
            l.start, l.end
        ));
        prev = next;
    }
    filter.push_str(&format!("[{prev}]format=yuv420p[vout]"));

    if !audio_labels.is_empty() {
        let inputs: String = audio_labels.iter().map(|l| format!("[{l}]")).collect();
        filter.push_str(&format!(
            ";{inputs}amix=inputs={}:normalize=0:duration=longest[aout]",
            audio_labels.len()
        ));
    }

    args.extend(["-filter_complex".into(), filter]);
    args.extend(["-map".into(), "[vout]".into()]);
    if !audio_labels.is_empty() {
        args.extend(["-map".into(), "[aout]".into()]);
        args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "192k".into()]);
    }
    match codec.as_str() {
        "h265" => args.extend([
            "-c:v".into(), "libx265".into(), "-tag:v".into(), "hvc1".into(),
            "-preset".into(), "medium".into(), "-crf".into(), "22".into(),
        ]),
        _ => args.extend([
            "-c:v".into(), "libx264".into(),
            "-preset".into(), "medium".into(), "-crf".into(), "18".into(),
        ]),
    }
    args.extend([
        "-t".into(), format!("{total_sec:.4}"),
        "-movflags".into(), "+faststart".into(),
        "-progress".into(), "pipe:1".into(),
        "-nostats".into(),
        output.clone(),
    ]);

    let mut child = Command::new("ffmpeg")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("ffmpeg not found — install FFmpeg and make sure it is on PATH")?;

    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();
    let total_us = total_sec * 1_000_000.0;
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(t) = line.strip_prefix("out_time_us=") {
            if let Ok(us) = t.trim().parse::<f64>() {
                let pct = (us / total_us * 100.0).clamp(0.0, 100.0);
                let _ = app.emit("export-progress", ExportProgress { percent: pct, done: false, error: None });
            }
        }
    }
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        let _ = app.emit("export-progress", ExportProgress { percent: 0.0, done: true, error: Some(err.clone()) });
        return Err(anyhow!("ffmpeg export failed: {err}"));
    }
    let _ = app.emit("export-progress", ExportProgress { percent: 100.0, done: true, error: None });
    Ok(())
}
