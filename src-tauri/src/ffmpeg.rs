use crate::model::{Clip, MediaAsset, MediaKind, Project, TrackKind};
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

/// Silence ranges of a source file segment, in SOURCE seconds.
pub async fn detect_silence(
    path: &str,
    from_sec: f64,
    dur_sec: f64,
    noise_db: f64,
    min_duration: f64,
) -> Result<Vec<(f64, f64)>> {
    let out = Command::new("ffmpeg")
        .args([
            "-v", "info",
            "-ss", &format!("{from_sec:.4}"),
            "-t", &format!("{dur_sec:.4}"),
            "-i", path,
            "-af", &format!("silencedetect=noise={noise_db}dB:d={min_duration}"),
            "-f", "null", "-",
        ])
        .output()
        .await
        .context("ffmpeg not found")?;
    let log = String::from_utf8_lossy(&out.stderr);
    let mut ranges = vec![];
    let mut start: Option<f64> = None;
    for line in log.lines() {
        if let Some(p) = line.find("silence_start: ") {
            start = line[p + 15..].split_whitespace().next().and_then(|s| s.parse().ok());
        } else if let Some(p) = line.find("silence_end: ") {
            if let (Some(s), Some(e)) = (
                start.take(),
                line[p + 13..].split_whitespace().next().and_then(|s| s.parse::<f64>().ok()),
            ) {
                // Offsets are relative to the segment; shift to absolute source time.
                ranges.push((from_sec + s, from_sec + e));
            }
        }
    }
    // Silence running to the end of the segment never gets a silence_end line.
    if let Some(s) = start {
        ranges.push((from_sec + s, from_sec + dur_sec));
    }
    Ok(ranges)
}

/// atempo only accepts 0.5–100 per instance; chain for slower rates.
fn atempo_chain(speed: f32) -> String {
    let mut s = speed as f64;
    let mut parts = vec![];
    while s < 0.5 {
        parts.push("atempo=0.5".to_string());
        s /= 0.5;
    }
    parts.push(format!("atempo={s:.4}"));
    parts.join(",")
}

fn eq_filter(c: &Clip) -> String {
    if c.brightness.abs() < 0.001 && (c.contrast - 1.0).abs() < 0.001 && (c.saturation - 1.0).abs() < 0.001 {
        String::new()
    } else {
        format!(
            "eq=brightness={}:contrast={}:saturation={},",
            c.brightness, c.contrast, c.saturation
        )
    }
}

fn audio_chain(c: &Clip, dur: f64, start: f64) -> String {
    let mut f = String::new();
    if (c.speed - 1.0).abs() > 0.001 {
        f.push_str(&atempo_chain(c.speed));
        f.push(',');
    }
    f.push_str(&format!("atrim=0:{dur:.4},asetpts=PTS-STARTPTS,"));
    if c.denoise {
        f.push_str("afftdn=nr=12,");
    }
    f.push_str(&format!("volume={}", c.volume));
    if c.fade_in > 0.01 {
        f.push_str(&format!(",afade=t=in:st=0:d={}", c.fade_in));
    }
    if c.fade_out > 0.01 {
        f.push_str(&format!(",afade=t=out:st={}:d={}", (dur - c.fade_out as f64).max(0.0), c.fade_out));
    }
    f.push_str(&format!(",adelay={}:all=1", (start * 1000.0).round() as i64));
    f
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
/// used). Per-clip chain: speed → scale into its slot box → color eq → alpha
/// fades → shift to timeline position. Video tracks composite bottom-to-top
/// with overlay at the clip's normalized center; audio is mixed with amix.
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

    struct VLayer {
        label: String,
        start: f64,
        end: f64,
        opacity: f32,
        pos_x: f32,
        pos_y: f32,
    }
    let mut video_layers: Vec<VLayer> = vec![]; // ordered bottom-to-top
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
            let src_dur = dur * clip.speed as f64;
            let box_w = ((w as f32 * clip.scale) as u32).max(2) & !1;
            let box_h = ((h as f32 * clip.scale) as u32).max(2) & !1;

            match (track.kind, media.kind) {
                (TrackKind::Video, MediaKind::Image) => {
                    args.extend(["-loop".into(), "1".into(), "-t".into(), format!("{dur:.4}")]);
                    args.extend(["-i".into(), media.path.clone()]);
                    let lbl = format!("v{input_idx}");
                    let mut fades = String::new();
                    if clip.fade_in > 0.01 {
                        fades.push_str(&format!("fade=t=in:st=0:d={}:alpha=1,", clip.fade_in));
                    }
                    if clip.fade_out > 0.01 {
                        fades.push_str(&format!(
                            "fade=t=out:st={}:d={}:alpha=1,",
                            (dur - clip.fade_out as f64).max(0.0),
                            clip.fade_out
                        ));
                    }
                    filter.push_str(&format!(
                        "[{input_idx}:v]scale={box_w}:{box_h}:force_original_aspect_ratio=decrease,\
                         setsar=1,fps={fps},{eq}format=yuva420p,{fades}\
                         setpts=PTS-STARTPTS+{start:.4}/TB[{lbl}];",
                        eq = eq_filter(clip),
                    ));
                    video_layers.push(VLayer {
                        label: lbl, start, end,
                        opacity: clip.opacity, pos_x: clip.pos_x, pos_y: clip.pos_y,
                    });
                    input_idx += 1;
                }
                (TrackKind::Video, MediaKind::Video) => {
                    args.extend([
                        "-ss".into(), format!("{:.4}", clip.source_in),
                        "-t".into(), format!("{src_dur:.4}"),
                        "-i".into(), media.path.clone(),
                    ]);
                    let lbl = format!("v{input_idx}");
                    let speed_f = if (clip.speed - 1.0).abs() > 0.001 {
                        format!("setpts=(PTS-STARTPTS)/{},", clip.speed)
                    } else {
                        String::new()
                    };
                    let mut fades = String::new();
                    if clip.fade_in > 0.01 {
                        fades.push_str(&format!("fade=t=in:st=0:d={}:alpha=1,", clip.fade_in));
                    }
                    if clip.fade_out > 0.01 {
                        fades.push_str(&format!(
                            "fade=t=out:st={}:d={}:alpha=1,",
                            (dur - clip.fade_out as f64).max(0.0),
                            clip.fade_out
                        ));
                    }
                    filter.push_str(&format!(
                        "[{input_idx}:v]{speed_f}scale={box_w}:{box_h}:force_original_aspect_ratio=decrease,\
                         setsar=1,fps={fps},{eq}format=yuva420p,{fades}\
                         setpts=PTS-STARTPTS+{start:.4}/TB[{lbl}];",
                        eq = eq_filter(clip),
                    ));
                    video_layers.push(VLayer {
                        label: lbl, start, end,
                        opacity: clip.opacity, pos_x: clip.pos_x, pos_y: clip.pos_y,
                    });
                    if media.has_audio && clip.volume > 0.0 {
                        let albl = format!("a{input_idx}");
                        filter.push_str(&format!(
                            "[{input_idx}:a]{}[{albl}];",
                            audio_chain(clip, dur, start)
                        ));
                        audio_labels.push(albl);
                    }
                    input_idx += 1;
                }
                (TrackKind::Audio, _) => {
                    args.extend([
                        "-ss".into(), format!("{:.4}", clip.source_in),
                        "-t".into(), format!("{src_dur:.4}"),
                        "-i".into(), media.path.clone(),
                    ]);
                    let albl = format!("a{input_idx}");
                    filter.push_str(&format!(
                        "[{input_idx}:a]{}[{albl}];",
                        audio_chain(clip, dur, start)
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
            "[{prev}][{src}]overlay=x=main_w*{px}-overlay_w/2:y=main_h*{py}-overlay_h/2:\
             enable='between(t,{s:.4},{e:.4})':eof_action=pass[{next}];",
            px = l.pos_x, py = l.pos_y, s = l.start, e = l.end
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
