import { findClip, useStore } from "../store";
import type { ClipProps } from "../types";

function Slider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  neutral?: number;
  onCommit: (v: number) => void;
}) {
  return (
    <div className="insp-row">
      <label>{props.label}</label>
      <input
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        defaultValue={props.value}
        key={`${props.label}-${props.value}`}
        onPointerUp={(e) => props.onCommit(parseFloat((e.target as HTMLInputElement).value))}
        onDoubleClick={() =>
          props.neutral !== undefined && props.onCommit(props.neutral)
        }
      />
      <span className="insp-val">{props.value.toFixed(2)}</span>
    </div>
  );
}

export default function Inspector() {
  const { project, selectedClipIds, setClipProps, applyLayout, removeSilence } = useStore();
  const clip = project && selectedClipIds.length ? findClip(project, selectedClipIds[0]) : undefined;

  if (!clip) {
    return (
      <aside className="inspector">
        <div className="panel-header"><span>Inspector</span></div>
        <p className="empty-hint">Select a clip to edit its properties.</p>
      </aside>
    );
  }

  const media = project!.media.find((m) => m.id === clip.media_id);
  const set = (props: ClipProps) => setClipProps(clip.id, props);
  const isVideoish = media?.kind !== "audio";
  const hasAudio = media?.kind === "audio" || media?.has_audio;

  return (
    <aside className="inspector">
      <div className="panel-header">
        <span>Inspector</span>
        <span className="insp-clipname">{media?.name}</span>
      </div>
      <div className="insp-body">
        {isVideoish && (
          <>
            <h4>Transform</h4>
            <Slider label="Scale" value={clip.scale} min={0.05} max={2} step={0.01} neutral={1} onCommit={(v) => set({ scale: v })} />
            <Slider label="X" value={clip.pos_x} min={0} max={1} step={0.01} neutral={0.5} onCommit={(v) => set({ pos_x: v })} />
            <Slider label="Y" value={clip.pos_y} min={0} max={1} step={0.01} neutral={0.5} onCommit={(v) => set({ pos_y: v })} />
            <div className="insp-row">
              <label>Layout</label>
              <select
                value=""
                onChange={(e) => {
                  if (e.target.value) applyLayout(selectedClipIds, e.target.value);
                }}
              >
                <option value="">apply…</option>
                <option value="fullscreen">Fullscreen</option>
                <option value="pip-top-left">PIP top-left</option>
                <option value="pip-top-right">PIP top-right</option>
                <option value="pip-bottom-left">PIP bottom-left</option>
                <option value="pip-bottom-right">PIP bottom-right</option>
                <option value="side-by-side">Side by side</option>
                <option value="thirds">Thirds</option>
                <option value="grid-2x2">Grid 2×2</option>
              </select>
            </div>
            <h4>Video</h4>
            <Slider label="Opacity" value={clip.opacity} min={0} max={1} step={0.01} neutral={1} onCommit={(v) => set({ opacity: v })} />
            <Slider label="Bright" value={clip.brightness} min={-1} max={1} step={0.01} neutral={0} onCommit={(v) => set({ brightness: v })} />
            <Slider label="Contrast" value={clip.contrast} min={0} max={2} step={0.01} neutral={1} onCommit={(v) => set({ contrast: v })} />
            <Slider label="Satur." value={clip.saturation} min={0} max={3} step={0.01} neutral={1} onCommit={(v) => set({ saturation: v })} />
          </>
        )}
        <h4>Timing</h4>
        <Slider label="Speed" value={clip.speed} min={0.25} max={4} step={0.05} neutral={1} onCommit={(v) => set({ speed: v })} />
        <Slider label="Fade in" value={clip.fade_in} min={0} max={5} step={0.1} neutral={0} onCommit={(v) => set({ fade_in: v })} />
        <Slider label="Fade out" value={clip.fade_out} min={0} max={5} step={0.1} neutral={0} onCommit={(v) => set({ fade_out: v })} />
        {hasAudio && (
          <>
            <h4>Audio</h4>
            <Slider label="Volume" value={clip.volume} min={0} max={2} step={0.01} neutral={1} onCommit={(v) => set({ volume: v })} />
            <div className="insp-row">
              <label>Denoise</label>
              <input
                type="checkbox"
                checked={clip.denoise}
                onChange={(e) => set({ denoise: e.target.checked })}
              />
              <span className="insp-val" />
            </div>
            <div className="insp-row">
              <button onClick={() => removeSilence(clip.id)} title="Detect silences and ripple-delete them">
                ✂ Remove silence
              </button>
            </div>
          </>
        )}
      </div>
    </aside>
  );
}
