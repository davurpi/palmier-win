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

  const expectedTime = clip ? clip.source_in + (playheadFrame - clip.start_frame) / fps : 0;

  useEffect(() => {
    const v = videoRef.current;
    if (!v || !clip) return;
    if (playing) {
      if (Math.abs(v.currentTime - expectedTime) > 0.2) v.currentTime = expectedTime;
      v.play().catch(() => {});
    } else {
      v.pause();
      v.currentTime = expectedTime;
    }
  }, [playing, clip?.id, media?.id]);

  // Re-sync on scrub while paused.
  useEffect(() => {
    const v = videoRef.current;
    if (!v || !clip || playing) return;
    if (Math.abs(v.currentTime - expectedTime) > 1 / fps) v.currentTime = expectedTime;
  }, [playheadFrame]);

  if (!clip || !media) return null;

  const style: React.CSSProperties = {
    zIndex,
    opacity: clip.opacity,
  };

  if (track.kind === "audio" || media.kind === "audio") {
    return (
      <video
        ref={videoRef}
        src={convertFileSrc(media.path)}
        style={{ display: "none" }}
        // volume must be set via effect on some webviews, but attribute works in practice
        onLoadedMetadata={(e) => (e.currentTarget.volume = Math.min(1, clip.volume))}
      />
    );
  }

  if (media.kind === "image") {
    return <img className="layer" src={convertFileSrc(media.path)} style={style} alt="" />;
  }

  return (
    <video
      ref={videoRef}
      className="layer"
      src={convertFileSrc(media.path)}
      style={style}
      onLoadedMetadata={(e) => (e.currentTarget.volume = Math.min(1, clip.volume))}
    />
  );
}
