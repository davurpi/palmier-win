import { convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useStore } from "../store";

export default function MediaBin() {
  const { project, importMedia } = useStore();

  const onImport = async () => {
    const paths = await open({
      multiple: true,
      filters: [
        {
          name: "Media",
          extensions: [
            "mp4", "mov", "mkv", "webm", "avi", "m4v",
            "mp3", "wav", "aac", "m4a", "flac", "ogg",
            "png", "jpg", "jpeg", "webp", "bmp", "gif",
          ],
        },
      ],
    });
    if (Array.isArray(paths) && paths.length) {
      await importMedia(paths);
    } else if (typeof paths === "string") {
      await importMedia([paths]);
    }
  };

  const fmtDur = (s: number) => {
    if (!s) return "";
    const m = Math.floor(s / 60);
    const sec = Math.floor(s % 60);
    return `${m}:${sec.toString().padStart(2, "0")}`;
  };

  return (
    <aside className="media-bin">
      <div className="panel-header">
        <span>Media</span>
        <button onClick={onImport}>+ Import</button>
      </div>
      <div className="media-grid">
        {project?.media.length === 0 && (
          <p className="empty-hint">
            Import video, audio or images — or let an AI agent do it through the
            MCP server.
          </p>
        )}
        {project?.media.map((m) => (
          <div
            key={m.id}
            className="media-card"
            draggable
            onDragStart={(e) => {
              e.dataTransfer.setData("application/x-media-id", m.id);
              e.dataTransfer.effectAllowed = "copy";
            }}
            title={`${m.name}\n${m.path}`}
          >
            <div className="media-thumb">
              {m.thumbnail ? (
                <img src={convertFileSrc(m.thumbnail)} alt="" draggable={false} />
              ) : (
                <span className="media-kind-icon">{m.kind === "audio" ? "♪" : "▶"}</span>
              )}
              {m.duration > 0 && <span className="media-dur">{fmtDur(m.duration)}</span>}
            </div>
            <div className="media-name">{m.name}</div>
          </div>
        ))}
      </div>
    </aside>
  );
}
