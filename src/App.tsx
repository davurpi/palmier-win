import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useStore } from "./store";
import MediaBin from "./components/MediaBin";
import Preview from "./components/Preview";
import Timeline from "./components/Timeline";
import ExportDialog from "./components/ExportDialog";
import Inspector from "./components/Inspector";
import type { Project } from "./types";
import "./App.css";

export default function App() {
  const {
    project, refresh, setProject, setExportOpen, splitAtPlayhead,
    removeSelected, playing, setPlaying, undo, redo,
  } = useStore();

  useEffect(() => {
    refresh();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (e.shiftKey) redo();
        else undo();
      } else if (e.key === " ") {
        e.preventDefault();
        setPlaying(!playing);
      } else if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        removeSelected();
      } else if (e.key.toLowerCase() === "s" && !e.metaKey && !e.ctrlKey) {
        splitAtPlayhead();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [playing, splitAtPlayhead, removeSelected, setPlaying, undo, redo]);

  const onNew = async () => {
    const p = await invoke<Project>("new_project");
    setProject(p);
  };

  const onOpen = async () => {
    const path = await open({
      multiple: false,
      filters: [{ name: "Palmier Win Project", extensions: ["palmierwin", "json"] }],
    });
    if (typeof path === "string") {
      const p = await invoke<Project>("load_project", { path });
      setProject(p);
    }
  };

  const onSave = async () => {
    let path = project?.path;
    if (!path) {
      const chosen = await save({
        defaultPath: `${project?.name ?? "Untitled"}.palmierwin`,
        filters: [{ name: "Palmier Win Project", extensions: ["palmierwin"] }],
      });
      if (!chosen) return;
      path = chosen;
    }
    await invoke("save_project", { path });
    await refresh();
  };

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <span className="brand-dot" /> Palmier&nbsp;<b>Win</b>
        </div>
        <div className="topbar-actions">
          <button onClick={onNew}>New</button>
          <button onClick={onOpen}>Open</button>
          <button onClick={onSave}>Save</button>
          <button className="primary" onClick={() => setExportOpen(true)}>
            Export
          </button>
        </div>
        <div className="topbar-meta">
          {project && (
            <span>
              {project.timeline.width}×{project.timeline.height} @ {project.timeline.fps}fps
            </span>
          )}
          <span className="mcp-badge" title="MCP server for AI agents (Claude, Cursor…)">
            MCP :19790
          </span>
        </div>
      </header>
      <div className="main">
        <MediaBin />
        <Preview />
        <Inspector />
      </div>
      <Timeline />
      <ExportDialog />
    </div>
  );
}
