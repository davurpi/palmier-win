import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useStore } from "../store";

export default function ExportDialog() {
  const { exportOpen, setExportOpen, exportProgress, project } = useStore();
  const [codec, setCodec] = useState<"h264" | "h265">("h264");
  const [running, setRunning] = useState(false);

  if (!exportOpen) return null;

  const empty = !project || project.timeline.tracks.every((t) => t.clips.length === 0);

  const onExport = async () => {
    const path = await save({
      defaultPath: `${project?.name ?? "export"}.mp4`,
      filters: [{ name: "MP4 video", extensions: ["mp4"] }],
    });
    if (!path) return;
    setRunning(true);
    await invoke("export_project", { outputPath: path, codec });
  };

  const done = exportProgress?.done && !exportProgress.error;
  const error = exportProgress?.error;

  return (
    <div className="modal-backdrop" onClick={() => !running && setExportOpen(false)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Export</h2>
        {empty ? (
          <p>The timeline is empty — add some clips first.</p>
        ) : (
          <>
            <div className="form-row">
              <label>Codec</label>
              <select value={codec} onChange={(e) => setCodec(e.target.value as any)}>
                <option value="h264">H.264 (compatible)</option>
                <option value="h265">H.265 / HEVC (smaller)</option>
              </select>
            </div>
            <div className="form-row">
              <label>Output</label>
              <span>
                {project.timeline.width}×{project.timeline.height} @ {project.timeline.fps}fps · AAC audio
              </span>
            </div>
            {running && exportProgress && !exportProgress.done && (
              <div className="progress">
                <div className="progress-bar" style={{ width: `${exportProgress.percent}%` }} />
                <span>{exportProgress.percent.toFixed(0)}%</span>
              </div>
            )}
            {done && <p className="ok">✓ Export finished.</p>}
            {error && <p className="err">Export failed: {error.slice(0, 400)}</p>}
          </>
        )}
        <div className="modal-actions">
          <button onClick={() => setExportOpen(false)}>Close</button>
          {!empty && (
            <button className="primary" onClick={onExport} disabled={running && !exportProgress?.done}>
              {running && !exportProgress?.done ? "Exporting…" : "Export MP4"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
