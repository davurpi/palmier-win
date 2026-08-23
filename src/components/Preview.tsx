import { useEffect, useRef } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useStore } from "../store";
import type { Clip, MediaAsset, Track } from "../types";

/**
 * Timeline preview: one layer per track, stacked in track order (index 0 on
 * top). Video/image layers composite visually; audio tracks play muted-hidden.
 * Export via FFmpeg is the exact render — this is a faithful approximation.
 */
export default function Preview() {
  const { project, playheadFrame, playing, setPlayhead, setPlaying } = useStore();
  const rafRef = useRef<number>(0);
  const lastTsRef = useRef<number>(0);

  const fps = project?.timeline.fps ?? 30;
  const durationFrames = project
    ? Math.max(0, ...project.timeline.tracks.flatMap((t) => t.clips.map((c) => c.end_frame)))
    : 0;

  useEffect(() => {
    if (!playing) return;
    lastTsRef.current = performance.now();
    const tick = (ts: number) => {
      const dt = (ts - lastTsRef.current) / 1000;
      lastTsRef.current = ts;
      const st = useStore.getState();
      const next = st.playheadFrame + dt * fps;
      if (next >= durationFrames) {
        st.setPlayhead(durationFrames);
        st.setPlaying(false);
        return;
      }
      st.setPlayhead(next);
      rafRef.current = requestAnimationFrame(tick);
    };
    rafRef.current = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(rafRef.current);
  }, [playing, fps, durationFrames]);

  const fmtTc = (frames: number) => {
    const total = frames / fps;
    const m = Math.floor(total / 60);
    const s = Math.floor(total % 60);
    const f = Math.floor(frames % fps);
    return `${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}.${f
      .toString()
      .padStart(2, "0")}`;
  };

  return (
    <section className="preview">
      <div className="preview-stage">
        <div
          className="preview-canvas"
          style={{ aspectRatio: `${project?.timeline.width ?? 16} / ${project?.timeline.height ?? 9}` }}
        >
          {project &&
            [...project.timeline.tracks]
              .reverse()
              .map((track, i) => (
                <TrackLayer key={track.id} track={track} zIndex={i} />
              ))}
        </div>
      </div>
      <div className="transport">
        <button onClick={() => setPlayhead(0)} title="Go to start">⏮</button>
        <button className="play-btn" onClick={() => setPlaying(!playing)}>
          {playing ? "⏸" : "▶"}
        </button>
        <span className="timecode">{fmtTc(playheadFrame)}</span>
      </div>
    </section>
  );
}

function activeClip(track: Track, frame: number): Clip | undefined {
  // Later clips in the array win, matching export order within a track.
  let found: Clip | undefined;
  for (const c of track.clips) {
    if (c.start_frame <= frame && frame < c.end_frame) found = c;
  }
  return found;
}

function TrackLayer({ track, zIndex }: { track: Track; zIndex: number }) {
  const { project, playheadFrame, playing } = useStore();
  const videoRef = useRef<HTMLVideoElement>(null);
  const fps = project?.timeline.fps ?? 30;

  const clip = activeClip(track, playheadFrame);
  const media: MediaAsset | undefined = clip
    ? project?.media.find((m) => m.id === clip.media_id)
    : undefined;

  const expectedTime = clip
    ? clip.source_in + ((playheadFrame - clip.start_frame) / fps) * clip.speed
    : 0;

  useEffect(() => {
    const v = videoRef.current;
    if (!v || !clip) return;
    v.playbackRate = clip.speed;
    if (playing) {
      if (Math.abs(v.currentTime - expectedTime) > 0.2) v.currentTime = expectedTime;
      v.play().catch(() => {});
    } else {
      v.pause();
      v.currentTime = expectedTime;
    }
  }, [playing, clip?.id, media?.id, clip?.speed]);

  // Re-sync on scrub while paused.
  useEffect(() => {
    const v = videoRef.current;
    if (!v || !clip || playing) return;
    if (Math.abs(v.currentTime - expectedTime) > 1 / fps) v.currentTime = expectedTime;
  }, [playheadFrame]);

  if (!clip || !media) return null;

  // Approximate fades in preview by ramping layer opacity.
  const tLocal = (playheadFrame - clip.start_frame) / fps;
  const tEnd = (clip.end_frame - clip.start_frame) / fps;
  let fadeMul = 1;
  if (clip.fade_in > 0.01 && tLocal < clip.fade_in) fadeMul = Math.max(0, tLocal / clip.fade_in);
  if (clip.fade_out > 0.01 && tEnd - tLocal < clip.fade_out)
    fadeMul = Math.min(fadeMul, Math.max(0, (tEnd - tLocal) / clip.fade_out));

  if (track.kind === "audio" || media.kind === "audio") {
    return (
      <video
        ref={videoRef}
        src={convertFileSrc(media.path)}
        style={{ display: "none" }}
        onLoadedMetadata={(e) =>
          (e.currentTarget.volume = Math.min(1, clip.volume * fadeMul))
        }
      />
    );
  }

  // Slot box centered at (pos_x, pos_y) sized scale× the canvas; media letterboxes inside.
  const wrapStyle: React.CSSProperties = {
    position: "absolute",
    left: `${clip.pos_x * 100}%`,
    top: `${clip.pos_y * 100}%`,
    width: `${clip.scale * 100}%`,
    height: `${clip.scale * 100}%`,
    transform: "translate(-50%, -50%)",
    zIndex,
    opacity: clip.opacity * fadeMul,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    pointerEvents: "none",
  };
  const mediaStyle: React.CSSProperties = {
    maxWidth: "100%",
    maxHeight: "100%",
    width: "100%",
    height: "100%",
    objectFit: "contain",
    filter: `brightness(${1 + clip.brightness}) contrast(${clip.contrast}) saturate(${clip.saturation})`,
  };

  if (media.kind === "image") {
    return (
      <div style={wrapStyle}>
        <img src={convertFileSrc(media.path)} style={mediaStyle} alt="" />
      </div>
    );
  }

  return (
    <div style={wrapStyle}>
      <video
        ref={videoRef}
        src={convertFileSrc(media.path)}
        style={mediaStyle}
        onLoadedMetadata={(e) =>
          (e.currentTarget.volume = Math.min(1, clip.volume * fadeMul))
        }
      />
    </div>
  );
}
