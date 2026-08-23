import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Clip, ClipProps, ExportProgress, Project } from "./types";

interface EditorState {
  project: Project | null;
  selectedClipIds: string[];
  playheadFrame: number;
  playing: boolean;
  pxPerFrame: number;
  exportProgress: ExportProgress | null;
  exportOpen: boolean;

  refresh: () => Promise<void>;
  setProject: (p: Project) => void;
  select: (ids: string[]) => void;
  setPlayhead: (f: number) => void;
  setPlaying: (p: boolean) => void;
  setZoom: (px: number) => void;
  setExportOpen: (open: boolean) => void;

  importMedia: (paths: string[]) => Promise<void>;
  addClip: (trackId: string, mediaId: string, startFrame: number) => Promise<void>;
  moveClip: (clipId: string, startFrame: number, trackId?: string) => Promise<void>;
  trimClip: (clipId: string, startFrame?: number, endFrame?: number) => Promise<void>;
  splitAtPlayhead: () => Promise<void>;
  removeSelected: () => Promise<void>;
  setClipProps: (clipId: string, props: ClipProps) => Promise<void>;
  addTrack: (kind: "video" | "audio") => Promise<void>;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  applyLayout: (clipIds: string[], layout: string) => Promise<void>;
  addText: (text: string, fontSize: number, color: string, posY: number) => Promise<void>;
  removeSilence: (clipId: string) => Promise<void>;
}

export function findClip(project: Project, clipId: string): Clip | undefined {
  for (const t of project.timeline.tracks) {
    const c = t.clips.find((c) => c.id === clipId);
    if (c) return c;
  }
  return undefined;
}

export const useStore = create<EditorState>((set, get) => ({
  project: null,
  selectedClipIds: [],
  playheadFrame: 0,
  playing: false,
  pxPerFrame: 2,
  exportProgress: null,
  exportOpen: false,

  refresh: async () => {
    const project = await invoke<Project>("get_project");
    set({ project });
  },
  setProject: (project) => set({ project }),
  select: (ids) => set({ selectedClipIds: ids }),
  setPlayhead: (f) => set({ playheadFrame: Math.max(0, f) }),
  setPlaying: (playing) => set({ playing }),
  setZoom: (px) => set({ pxPerFrame: Math.min(20, Math.max(0.05, px)) }),
  setExportOpen: (exportOpen) => set({ exportOpen, exportProgress: exportOpen ? null : get().exportProgress }),

  importMedia: async (paths) => {
    await invoke("import_media", { paths });
    await get().refresh();
  },
  addClip: async (trackId, mediaId, startFrame) => {
    await invoke("add_clip", { trackId, mediaId, startFrame });
    await get().refresh();
  },
  moveClip: async (clipId, startFrame, trackId) => {
    await invoke("move_clip", { clipId, startFrame, trackId: trackId ?? null });
    await get().refresh();
  },
  trimClip: async (clipId, startFrame, endFrame) => {
    await invoke("trim_clip", {
      clipId,
      startFrame: startFrame ?? null,
      endFrame: endFrame ?? null,
    });
    await get().refresh();
  },
  splitAtPlayhead: async () => {
    const { project, playheadFrame, selectedClipIds } = get();
    if (!project) return;
    const targets = selectedClipIds.length
      ? selectedClipIds
      : project.timeline.tracks
          .flatMap((t) => t.clips)
          .filter((c) => c.start_frame < playheadFrame && playheadFrame < c.end_frame)
          .map((c) => c.id);
    for (const id of targets) {
      const clip = findClip(project, id);
      if (clip && clip.start_frame < playheadFrame && playheadFrame < clip.end_frame) {
        await invoke("split_clip", { clipId: id, frame: playheadFrame });
      }
    }
    await get().refresh();
  },
  removeSelected: async () => {
    const ids = get().selectedClipIds;
    if (!ids.length) return;
    await invoke("remove_clips", { clipIds: ids });
    set({ selectedClipIds: [] });
    await get().refresh();
  },
  setClipProps: async (clipId, props) => {
    await invoke("set_clip_properties", { clipId, props });
    await get().refresh();
  },
  addTrack: async (kind) => {
    await invoke("add_track", { kind });
    await get().refresh();
  },
  undo: async () => {
    try {
      await invoke("undo");
      await get().refresh();
    } catch {
      /* nothing to undo */
    }
  },
  redo: async () => {
    try {
      await invoke("redo");
      await get().refresh();
    } catch {
      /* nothing to redo */
    }
  },
  applyLayout: async (clipIds, layout) => {
    await invoke("apply_layout", { clipIds, layout });
    await get().refresh();
  },
  addText: async (text, fontSize, color, posY) => {
    await invoke("add_text", {
      trackId: null,
      text,
      startFrame: Math.round(get().playheadFrame),
      durationFrames: null,
      fontSize,
      color,
      posX: null,
      posY,
    });
    await get().refresh();
  },
  removeSilence: async (clipId) => {
    await invoke("remove_silence", { clipId, noiseDb: null, minDuration: null });
    set({ selectedClipIds: [] });
    await get().refresh();
  },
}));

// Global event wiring: MCP mutations and export progress from the Rust side.
listen<Project>("project-changed", (e) => {
  useStore.setState({ project: e.payload });
});
listen<ExportProgress>("export-progress", (e) => {
  useStore.setState({ exportProgress: e.payload });
});
