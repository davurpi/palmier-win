use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn new_id() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Video,
    Audio,
    Image,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MediaAsset {
    pub id: String,
    pub path: String,
    pub name: String,
    pub kind: MediaKind,
    /// Duration in seconds (images report 0).
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
    /// Absolute path of a generated thumbnail PNG, if any.
    pub thumbnail: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Video,
    Audio,
}

fn d_half() -> f32 { 0.5 }
fn d_one() -> f32 { 1.0 }

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Clip {
    pub id: String,
    pub media_id: String,
    /// Timeline placement in frames, [start, end).
    pub start_frame: i64,
    pub end_frame: i64,
    /// Offset into the source media, in seconds.
    pub source_in: f64,
    pub volume: f32,
    pub opacity: f32,
    /// Canvas-normalized center position (0..1) and uniform scale (1 = fit canvas).
    #[serde(default = "d_half")]
    pub pos_x: f32,
    #[serde(default = "d_half")]
    pub pos_y: f32,
    #[serde(default = "d_one")]
    pub scale: f32,
    /// Playback rate: 1 = normal, 2 = double speed. Timeline length stays fixed;
    /// speed changes how much source is consumed.
    #[serde(default = "d_one")]
    pub speed: f32,
    /// Fade in/out duration in seconds (video alpha + audio).
    #[serde(default)]
    pub fade_in: f32,
    #[serde(default)]
    pub fade_out: f32,
    #[serde(default)]
    pub denoise: bool,
    /// Color: brightness -1..1 (0 neutral), contrast 0..2 (1), saturation 0..3 (1).
    #[serde(default)]
    pub brightness: f32,
    #[serde(default = "d_one")]
    pub contrast: f32,
    #[serde(default = "d_one")]
    pub saturation: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Track {
    pub id: String,
    pub kind: TrackKind,
    pub name: String,
    pub clips: Vec<Clip>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Timeline {
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    pub tracks: Vec<Track>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Project {
    pub name: String,
    pub path: Option<String>,
    pub media: Vec<MediaAsset>,
    pub timeline: Timeline,
}

impl Default for Project {
    fn default() -> Self {
        Project {
            name: "Untitled".into(),
            path: None,
            media: vec![],
            timeline: Timeline {
                fps: 30.0,
                width: 1920,
                height: 1080,
                tracks: vec![
                    Track { id: new_id(), kind: TrackKind::Video, name: "V2".into(), clips: vec![] },
                    Track { id: new_id(), kind: TrackKind::Video, name: "V1".into(), clips: vec![] },
                    Track { id: new_id(), kind: TrackKind::Audio, name: "A1".into(), clips: vec![] },
                ],
            },
        }
    }
}

impl Project {
    pub fn media_by_id(&self, id: &str) -> Option<&MediaAsset> {
        self.media.iter().find(|m| m.id == id)
    }

    pub fn duration_frames(&self) -> i64 {
        self.timeline
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(|c| c.end_frame))
            .max()
            .unwrap_or(0)
    }

    fn find_clip_mut(&mut self, clip_id: &str) -> Option<(usize, usize)> {
        for (ti, t) in self.timeline.tracks.iter().enumerate() {
            if let Some(ci) = t.clips.iter().position(|c| c.id == clip_id) {
                return Some((ti, ci));
            }
        }
        None
    }

    pub fn add_track(&mut self, kind: TrackKind) -> Result<Track, String> {
        let n = self.timeline.tracks.iter().filter(|t| t.kind == kind).count() + 1;
        let prefix = match kind {
            TrackKind::Video => "V",
            TrackKind::Audio => "A",
        };
        let track = Track {
            id: new_id(),
            kind,
            name: format!("{prefix}{n}"),
            clips: vec![],
        };
        // New video tracks go on top (index 0); audio tracks at the bottom.
        match kind {
            TrackKind::Video => self.timeline.tracks.insert(0, track.clone()),
            TrackKind::Audio => self.timeline.tracks.push(track.clone()),
        }
        Ok(track)
    }

    pub fn remove_track(&mut self, track_id: &str) -> Result<(), String> {
        let before = self.timeline.tracks.len();
        self.timeline.tracks.retain(|t| t.id != track_id);
        if self.timeline.tracks.len() == before {
            return Err(format!("track {track_id} not found"));
        }
        Ok(())
    }

    pub fn add_clip(
        &mut self,
        track_id: &str,
        media_id: &str,
        start_frame: i64,
        duration_frames: Option<i64>,
        source_in: Option<f64>,
    ) -> Result<Clip, String> {
        let fps = self.timeline.fps;
        let media = self
            .media_by_id(media_id)
            .ok_or_else(|| format!("media {media_id} not found"))?
            .clone();
        let track = self
            .timeline
            .tracks
            .iter_mut()
            .find(|t| t.id == track_id)
            .ok_or_else(|| format!("track {track_id} not found"))?;
        if track.kind == TrackKind::Audio && media.kind == MediaKind::Image {
            return Err("cannot place an image on an audio track".into());
        }
        if track.kind == TrackKind::Audio && !media.has_audio {
            return Err("media has no audio stream".into());
        }
        let src_in = source_in.unwrap_or(0.0);
        let natural = if media.kind == MediaKind::Image {
            (5.0 * fps) as i64
        } else {
            (((media.duration - src_in).max(0.1)) * fps).round() as i64
        };
        let dur = duration_frames.unwrap_or(natural).max(1);
        let clip = Clip {
            id: new_id(),
            media_id: media_id.to_string(),
            start_frame: start_frame.max(0),
            end_frame: start_frame.max(0) + dur,
            source_in: src_in,
            volume: 1.0,
            opacity: 1.0,
            pos_x: 0.5,
            pos_y: 0.5,
            scale: 1.0,
            speed: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
            denoise: false,
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
        };
        track.clips.push(clip.clone());
        Ok(clip)
    }

    pub fn move_clip(
        &mut self,
        clip_id: &str,
        start_frame: i64,
        target_track_id: Option<String>,
    ) -> Result<Clip, String> {
        let (ti, ci) = self
            .find_clip_mut(clip_id)
            .ok_or_else(|| format!("clip {clip_id} not found"))?;
        let mut clip = self.timeline.tracks[ti].clips.remove(ci);
        let dur = clip.end_frame - clip.start_frame;
        clip.start_frame = start_frame.max(0);
        clip.end_frame = clip.start_frame + dur;
        let dest_ti = match target_track_id {
            Some(id) => self
                .timeline
                .tracks
                .iter()
                .position(|t| t.id == id)
                .ok_or_else(|| format!("track {id} not found"))?,
            None => ti,
        };
        self.timeline.tracks[dest_ti].clips.push(clip.clone());
        Ok(clip)
    }

    pub fn trim_clip(
        &mut self,
        clip_id: &str,
        start_frame: Option<i64>,
        end_frame: Option<i64>,
    ) -> Result<Clip, String> {
        let fps = self.timeline.fps;
        let (ti, ci) = self
            .find_clip_mut(clip_id)
            .ok_or_else(|| format!("clip {clip_id} not found"))?;
        let clip = &mut self.timeline.tracks[ti].clips[ci];
        if let Some(s) = start_frame {
            let s = s.clamp(0, clip.end_frame - 1);
            // Trimming the head moves source_in so content stays in place.
            clip.source_in =
                (clip.source_in + (s - clip.start_frame) as f64 / fps * clip.speed as f64).max(0.0);
            clip.start_frame = s;
        }
        if let Some(e) = end_frame {
            clip.end_frame = e.max(clip.start_frame + 1);
        }
        Ok(clip.clone())
    }

    pub fn split_clip(&mut self, clip_id: &str, at_frame: i64) -> Result<Vec<Clip>, String> {
        let fps = self.timeline.fps;
        let (ti, ci) = self
            .find_clip_mut(clip_id)
            .ok_or_else(|| format!("clip {clip_id} not found"))?;
        let clip = self.timeline.tracks[ti].clips[ci].clone();
        if at_frame <= clip.start_frame || at_frame >= clip.end_frame {
            return Err("split point is outside the clip".into());
        }
        let mut left = clip.clone();
        left.end_frame = at_frame;
        let mut right = clip.clone();
        right.id = new_id();
        right.start_frame = at_frame;
        right.source_in =
            clip.source_in + (at_frame - clip.start_frame) as f64 / fps * clip.speed as f64;
        self.timeline.tracks[ti].clips[ci] = left.clone();
        self.timeline.tracks[ti].clips.push(right.clone());
        Ok(vec![left, right])
    }

    pub fn remove_clips(&mut self, ids: &[String]) -> usize {
        let mut removed = 0;
        for t in &mut self.timeline.tracks {
            let before = t.clips.len();
            t.clips.retain(|c| !ids.contains(&c.id));
            removed += before - t.clips.len();
        }
        removed
    }

    pub fn set_clip_properties(&mut self, clip_id: &str, p: ClipProps) -> Result<Clip, String> {
        let (ti, ci) = self
            .find_clip_mut(clip_id)
            .ok_or_else(|| format!("clip {clip_id} not found"))?;
        let clip = &mut self.timeline.tracks[ti].clips[ci];
        if let Some(v) = p.volume { clip.volume = v.clamp(0.0, 4.0); }
        if let Some(o) = p.opacity { clip.opacity = o.clamp(0.0, 1.0); }
        if let Some(x) = p.pos_x { clip.pos_x = x.clamp(-0.5, 1.5); }
        if let Some(y) = p.pos_y { clip.pos_y = y.clamp(-0.5, 1.5); }
        if let Some(s) = p.scale { clip.scale = s.clamp(0.05, 4.0); }
        if let Some(s) = p.speed { clip.speed = s.clamp(0.25, 4.0); }
        if let Some(f) = p.fade_in { clip.fade_in = f.clamp(0.0, 30.0); }
        if let Some(f) = p.fade_out { clip.fade_out = f.clamp(0.0, 30.0); }
        if let Some(d) = p.denoise { clip.denoise = d; }
        if let Some(b) = p.brightness { clip.brightness = b.clamp(-1.0, 1.0); }
        if let Some(c) = p.contrast { clip.contrast = c.clamp(0.0, 2.0); }
        if let Some(s) = p.saturation { clip.saturation = s.clamp(0.0, 3.0); }
        Ok(clip.clone())
    }

    /// Assign layout slots (position + scale) to clips, in order.
    pub fn apply_layout(&mut self, clip_ids: &[String], layout: &str) -> Result<Vec<Clip>, String> {
        let slots: Vec<(f32, f32, f32)> = match layout {
            "fullscreen" => vec![(0.5, 0.5, 1.0)],
            "pip-top-left" => vec![(0.5, 0.5, 1.0), (0.18, 0.18, 0.32)],
            "pip-top-right" => vec![(0.5, 0.5, 1.0), (0.82, 0.18, 0.32)],
            "pip-bottom-left" => vec![(0.5, 0.5, 1.0), (0.18, 0.82, 0.32)],
            "pip-bottom-right" => vec![(0.5, 0.5, 1.0), (0.82, 0.82, 0.32)],
            "side-by-side" => vec![(0.25, 0.5, 0.5), (0.75, 0.5, 0.5)],
            "thirds" => vec![(0.1667, 0.5, 0.3333), (0.5, 0.5, 0.3333), (0.8333, 0.5, 0.3333)],
            "grid-2x2" => vec![
                (0.25, 0.25, 0.5), (0.75, 0.25, 0.5),
                (0.25, 0.75, 0.5), (0.75, 0.75, 0.5),
            ],
            _ => return Err(format!(
                "unknown layout {layout} (use fullscreen, pip-top-left, pip-top-right, pip-bottom-left, pip-bottom-right, side-by-side, thirds, grid-2x2)"
            )),
        };
        if clip_ids.len() > slots.len() {
            return Err(format!("layout {layout} has {} slots, got {} clips", slots.len(), clip_ids.len()));
        }
        let mut out = vec![];
        for (id, (x, y, s)) in clip_ids.iter().zip(slots) {
            let (ti, ci) = self
                .find_clip_mut(id)
                .ok_or_else(|| format!("clip {id} not found"))?;
            let clip = &mut self.timeline.tracks[ti].clips[ci];
            clip.pos_x = x;
            clip.pos_y = y;
            clip.scale = s;
            out.push(clip.clone());
        }
        Ok(out)
    }

    /// Delete [start, end) across every track and close the gap (ripple).
    pub fn ripple_delete_range(&mut self, start: i64, end: i64) -> Result<(), String> {
        if end <= start {
            return Err("end must be greater than start".into());
        }
        let len = end - start;
        let fps = self.timeline.fps;
        for track in &mut self.timeline.tracks {
            let mut new_clips = vec![];
            for mut clip in track.clips.drain(..) {
                let speed = clip.speed as f64;
                if clip.end_frame <= start {
                    new_clips.push(clip); // fully before
                } else if clip.start_frame >= end {
                    clip.start_frame -= len; // fully after: shift left
                    clip.end_frame -= len;
                    new_clips.push(clip);
                } else if clip.start_frame >= start && clip.end_frame <= end {
                    // fully inside: dropped
                } else if clip.start_frame < start && clip.end_frame > end {
                    // spans the range: split into head + tail, tail shifts onto the cut
                    let mut tail = clip.clone();
                    tail.id = new_id();
                    tail.source_in =
                        clip.source_in + (end - clip.start_frame) as f64 / fps * speed;
                    tail.start_frame = start;
                    tail.end_frame = start + (clip.end_frame - end);
                    clip.end_frame = start;
                    new_clips.push(clip);
                    new_clips.push(tail);
                } else if clip.start_frame < start {
                    clip.end_frame = start; // overlaps head of range
                    new_clips.push(clip);
                } else {
                    // overlaps tail of range: trim front, shift onto the cut
                    clip.source_in +=
                        (end - clip.start_frame) as f64 / fps * speed;
                    let dur = clip.end_frame - end;
                    clip.start_frame = start;
                    clip.end_frame = start + dur;
                    new_clips.push(clip);
                }
            }
            track.clips = new_clips;
        }
        Ok(())
    }
}

#[derive(Deserialize, Default, Clone, Debug)]
pub struct ClipProps {
    pub volume: Option<f32>,
    pub opacity: Option<f32>,
    pub pos_x: Option<f32>,
    pub pos_y: Option<f32>,
    pub scale: Option<f32>,
    pub speed: Option<f32>,
    pub fade_in: Option<f32>,
    pub fade_out: Option<f32>,
    pub denoise: Option<bool>,
    pub brightness: Option<f32>,
    pub contrast: Option<f32>,
    pub saturation: Option<f32>,
}
