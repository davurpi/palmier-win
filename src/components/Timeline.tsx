import { useRef, useState } from "react";
import { useStore } from "../store";
import type { Clip, Track } from "../types";

function TextDialog({ onClose }: { onClose: () => void }) {
  const { addText } = useStore();
  const [text, setText] = useState("");
  const [size, setSize] = useState(72);
  const [color, setColor] = useState("#ffffff");
  const [posY, setPosY] = useState(0.5);
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Add text</h2>
        <div className="form-row">
          <label>Text</label>
          <textarea
            autoFocus
            rows={2}
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="Your title…"
          />
        </div>
        <div className="form-row">
          <label>Size</label>
          <input type="number" min={16} max={300} value={size} onChange={(e) => setSize(+e.target.value)} />
          <label>Color</label>
          <input type="color" value={color} onChange={(e) => setColor(e.target.value)} />
        </div>
        <div className="form-row">
          <label>Position</label>
          <select value={posY} onChange={(e) => setPosY(+e.target.value)}>
            <option value={0.15}>Top</option>
            <option value={0.5}>Center</option>
            <option value={0.8}>Lower third</option>
          </select>
        </div>
        <div className="modal-actions">
          <button onClick={onClose}>Cancel</button>
          <button
            className="primary"
            disabled={!text.trim()}
            onClick={async () => {
              await addText(text, size, color, posY);
              onClose();
            }}
          >
            Add at playhead
          </button>
        </div>
      </div>
    </div>
  );
}

interface DragState {
  clipId: string;
  mode: "move" | "trim-l" | "trim-r";
  startX: number;
  deltaFrames: number;
  targetTrackId: string | null;
}

export default function Timeline() {
  const {
    project, pxPerFrame, setZoom, playheadFrame, setPlayhead,
    selectedClipIds, select, addClip, moveClip, trimClip, addTrack,
    splitAtPlayhead, removeSelected, undo, redo,
  } = useStore();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<DragState | null>(null);
  const [textOpen, setTextOpen] = useState(false);

  if (!project) return <section className="timeline" />;
  const fps = project.timeline.fps;
  const durationFrames = Math.max(
    0,
    ...project.timeline.tracks.flatMap((t) => t.clips.map((c) => c.end_frame))
  );
  const totalFrames = Math.max(durationFrames + 5 * fps, 30 * fps);
  const totalPx = totalFrames * pxPerFrame;

  const frameFromClientX = (clientX: number) => {
    const el = scrollRef.current;
    if (!el) return 0;
    const rect = el.getBoundingClientRect();
    const x = clientX - rect.left + el.scrollLeft - LABEL_W;
    return Math.max(0, Math.round(x / pxPerFrame));
  };

  const onRulerPointerDown = (e: React.PointerEvent) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    setPlayhead(frameFromClientX(e.clientX));
    const onMove = (ev: PointerEvent) => setPlayhead(frameFromClientX(ev.clientX));
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  const onWheel = (e: React.WheelEvent) => {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      setZoom(pxPerFrame * (e.deltaY < 0 ? 1.2 : 1 / 1.2));
    }
  };

  // Ruler tick step: first step that is at least ~70px wide.
  const stepsSec = [0.2, 0.5, 1, 2, 5, 10, 30, 60];
  const stepSec = stepsSec.find((s) => s * fps * pxPerFrame >= 70) ?? 60;
  const ticks: number[] = [];
  for (let t = 0; t <= totalFrames / fps; t += stepSec) ticks.push(t);

  const fmt = (sec: number) => {
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s < 10 ? "0" : ""}${Number.isInteger(s) ? s : s.toFixed(1)}`;
  };

  return (
    <section className="timeline" onWheel={onWheel}>
      <div className="timeline-toolbar">
        <button onClick={undo} title="Undo (Ctrl/Cmd+Z)">↩</button>
        <button onClick={redo} title="Redo (Ctrl/Cmd+Shift+Z)">↪</button>
        <span className="spacer" />
        <button onClick={() => addTrack("video")}>+ Video track</button>
        <button onClick={() => addTrack("audio")}>+ Audio track</button>
        <button onClick={() => setTextOpen(true)}>+ Text</button>
        <span className="spacer" />
        <button onClick={splitAtPlayhead} title="Split at playhead (S)">✂ Split</button>
        <button onClick={removeSelected} title="Delete selected (Del)">🗑 Delete</button>
        <span className="spacer" />
        <button onClick={() => setZoom(pxPerFrame / 1.4)}>−</button>
        <span className="zoom-label">zoom</span>
        <button onClick={() => setZoom(pxPerFrame * 1.4)}>+</button>
      </div>
      <div className="timeline-scroll" ref={scrollRef}>
        <div className="timeline-inner" style={{ width: totalPx + LABEL_W }}>
          <div className="ruler" onPointerDown={onRulerPointerDown}>
            {ticks.map((t) => (
              <span key={t} className="tick" style={{ left: LABEL_W + t * fps * pxPerFrame }}>
                {fmt(t)}
              </span>
            ))}
          </div>
          {project.timeline.tracks.map((track) => (
            <TrackRow
              key={track.id}
              track={track}
              pxPerFrame={pxPerFrame}
              drag={drag}
              setDrag={setDrag}
              selectedClipIds={selectedClipIds}
              select={select}
              onDropMedia={(mediaId, frame) => addClip(track.id, mediaId, frame)}
              onCommitMove={(clipId, frames, trackId) => moveClip(clipId, frames, trackId)}
              onCommitTrim={(clipId, s, e2) => trimClip(clipId, s, e2)}
              frameFromClientX={frameFromClientX}
            />
          ))}
          <div
            className="playhead"
            style={{ left: LABEL_W + playheadFrame * pxPerFrame }}
          />
        </div>
      </div>
      {textOpen && <TextDialog onClose={() => setTextOpen(false)} />}
    </section>
  );
}

const LABEL_W = 72;

function TrackRow(props: {
  track: Track;
  pxPerFrame: number;
  drag: DragState | null;
  setDrag: (d: DragState | null) => void;
  selectedClipIds: string[];
  select: (ids: string[]) => void;
  onDropMedia: (mediaId: string, frame: number) => void;
  onCommitMove: (clipId: string, startFrame: number, trackId?: string) => void;
  onCommitTrim: (clipId: string, start?: number, end?: number) => void;
  frameFromClientX: (x: number) => number;
}) {
  const { track, pxPerFrame, drag, setDrag, selectedClipIds, select } = props;
  const { project } = useStore.getState();

  const onPointerDown = (e: React.PointerEvent, clip: Clip, mode: DragState["mode"]) => {
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    select([clip.id]);
    const startX = e.clientX;
    const state: DragState = { clipId: clip.id, mode, startX, deltaFrames: 0, targetTrackId: null };
    setDrag(state);

    // Snap targets: other clips' edges, the playhead and frame 0.
    const st = useStore.getState();
    const candidates: number[] = [0, Math.round(st.playheadFrame)];
    st.project?.timeline.tracks.forEach((t) =>
      t.clips.forEach((c) => {
        if (c.id !== clip.id) candidates.push(c.start_frame, c.end_frame);
      })
    );
    const dur = clip.end_frame - clip.start_frame;
    const snapMove = (raw: number) => {
      const th = 8 / pxPerFrame;
      let best = raw;
      let bestD = th;
      for (const c of candidates) {
        const dS = Math.abs(c - (clip.start_frame + raw));
        if (dS < bestD) { bestD = dS; best = c - clip.start_frame; }
        const dE = Math.abs(c - (clip.start_frame + raw + dur));
        if (dE < bestD) { bestD = dE; best = c - clip.start_frame - dur; }
      }
      return Math.round(best);
    };

    const onMove = (ev: PointerEvent) => {
      let deltaFrames = Math.round((ev.clientX - startX) / pxPerFrame);
      if (mode === "move") deltaFrames = snapMove(deltaFrames);
      let targetTrackId: string | null = null;
      if (mode === "move") {
        const lane = document
          .elementsFromPoint(ev.clientX, ev.clientY)
          .find((el) => el.classList.contains("track-lane")) as HTMLElement | undefined;
        if (lane && lane.dataset.kind === track.kind && lane.dataset.trackId !== track.id) {
          targetTrackId = lane.dataset.trackId ?? null;
        }
      }
      setDrag({ ...state, deltaFrames, targetTrackId });
    };
    const onUp = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      let deltaFrames = Math.round((ev.clientX - startX) / pxPerFrame);
      if (mode === "move") deltaFrames = snapMove(deltaFrames);
      setDrag(null);
      if (mode === "move") {
        const lane = document
          .elementsFromPoint(ev.clientX, ev.clientY)
          .find((el) => el.classList.contains("track-lane")) as HTMLElement | undefined;
        const tid =
          lane && lane.dataset.kind === track.kind && lane.dataset.trackId !== track.id
            ? lane.dataset.trackId
            : undefined;
        if (deltaFrames !== 0 || tid) {
          props.onCommitMove(clip.id, Math.max(0, clip.start_frame + deltaFrames), tid);
        }
      } else if (mode === "trim-l" && deltaFrames !== 0) {
        props.onCommitTrim(clip.id, Math.max(0, clip.start_frame + deltaFrames), undefined);
      } else if (mode === "trim-r" && deltaFrames !== 0) {
        props.onCommitTrim(clip.id, undefined, Math.max(clip.start_frame + 1, clip.end_frame + deltaFrames));
      }
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  return (
    <div className={`track-row kind-${track.kind}`}>
      <div className="track-label">{track.name}</div>
      <div
        className="track-lane"
        data-track-id={track.id}
        data-kind={track.kind}
        onDragOver={(e) => {
          if (e.dataTransfer.types.includes("application/x-media-id")) e.preventDefault();
        }}
        onDrop={(e) => {
          const mediaId = e.dataTransfer.getData("application/x-media-id");
          if (mediaId) props.onDropMedia(mediaId, props.frameFromClientX(e.clientX));
        }}
        onPointerDown={() => select([])}
      >
        {track.clips.map((clip) => {
          const isDragging = drag?.clipId === clip.id;
          let start = clip.start_frame;
          let end = clip.end_frame;
          if (isDragging && drag) {
            if (drag.mode === "move") {
              start += drag.deltaFrames;
              end += drag.deltaFrames;
            } else if (drag.mode === "trim-l") {
              start = Math.min(end - 1, start + drag.deltaFrames);
            } else if (drag.mode === "trim-r") {
              end = Math.max(start + 1, end + drag.deltaFrames);
            }
          }
          const media = project?.media.find((m) => m.id === clip.media_id);
          return (
            <div
              key={clip.id}
              className={
                "clip" +
                (selectedClipIds.includes(clip.id) ? " selected" : "") +
                (isDragging ? " dragging" : "") +
                ` media-${media?.kind ?? "video"}`
              }
              style={{
                left: Math.max(0, start) * pxPerFrame,
                width: Math.max(2, (end - Math.max(0, start)) * pxPerFrame),
              }}
              onPointerDown={(e) => onPointerDown(e, clip, "move")}
            >
              <div
                className="trim-handle left"
                onPointerDown={(e) => onPointerDown(e, clip, "trim-l")}
              />
              <span className="clip-name">{media?.name ?? clip.media_id}</span>
              <div
                className="trim-handle right"
                onPointerDown={(e) => onPointerDown(e, clip, "trim-r")}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}
