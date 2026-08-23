# Palmier Win

**The video editor built for AI — on Windows (and macOS).**

An independent, cross-platform reimplementation of the concept behind
[Palmier Pro](https://github.com/palmier-io/palmier-pro), which is a
macOS-26-only Swift app. Palmier Win rebuilds the two core ideas —
a real timeline editor **and** an MCP server so AI agents (Claude, Cursor,
Codex…) can edit alongside you — on a stack that runs on Windows:

| | Palmier Pro (macOS) | Palmier Win |
|---|---|---|
| UI | SwiftUI/AppKit | Tauri 2 + React (WebView2/DirectX on Windows) |
| Video engine | AVFoundation + Metal | **FFmpeg** (decode, compositing, encode) |
| AI control | MCP server | MCP server on `http://127.0.0.1:19790/mcp` |
| License | GPLv3 (core) | GPLv3 |

## Features (v0.2)

- Project management (`.palmierwin` JSON projects), New/Open/Save, **undo/redo**
  (Ctrl/Cmd+Z, 100 levels)
- Import video, audio and images (ffprobe metadata + thumbnails)
- Multi-track timeline: drag with **edge/playhead snapping**, move across
  tracks, trim, razor split (S), delete (Del), zoom (Ctrl+wheel)
- **Inspector panel**: transform (position/scale), layouts (**PIP, side by
  side, thirds, 2×2 grid**), opacity, **brightness/contrast/saturation**,
  **speed (0.25–4×)**, **fade in/out**, volume, **audio denoise**
- **Text overlays** rendered with system fonts (outlined, multi-line,
  lower-thirds)
- **Remove silence**: FFmpeg silencedetect + ripple delete, one click
- **Ripple delete ranges** across all tracks
- Composited preview player honoring transforms, color, fades and speed
- Export to H.264 / H.265 MP4 with AAC audio via FFmpeg `filter_complex`,
  with live progress
- **MCP server** with 18 tools: `get_project`, `import_media`, `add_track`,
  `add_clip`, `move_clip`, `trim_clip`, `split_clip`, `remove_clips`,
  `set_clip_properties`, `apply_layout`, `add_text`, `ripple_delete_ranges`,
  `remove_silence`, `undo`, `redo`, `set_timeline_settings`, `export_project`

Not yet (vs. Palmier Pro on macOS): transcription/captions & word-based cuts
(needs Whisper), beat detection, multicam, nested/multiple timelines,
keyframes, XML/FCPXML interchange, cloud generation.

## Requirements

- **FFmpeg + FFprobe on PATH** (the whole engine).
  - Windows: `winget install ffmpeg` (or a gpl build from gyan.dev)
  - macOS: `brew install ffmpeg`
- To build: Node 20+, Rust 1.77+, and on Windows the
  [Tauri prerequisites](https://tauri.app/start/prerequisites/)
  (VS Build Tools C++, WebView2 runtime).

## Develop

```bash
npm install
npm run tauri dev
```

## Build installers

```bash
npm run tauri build
```

On Windows this produces an NSIS `.exe` and an `.msi` under
`src-tauri/target/release/bundle/`. The GitHub Actions workflow in
`.github/workflows/build.yml` builds Windows + macOS installers on every
push and attaches them to a draft release on `v*` tags.

## Connect Claude (MCP)

With the app running:

```bash
claude mcp add --transport http palmier-win http://127.0.0.1:19790/mcp
```

Then ask Claude to import media, build the timeline and export — every
change shows up live in the app window.

## Architecture

```
src/                    React UI (media bin, preview, timeline, export)
src-tauri/src/model.rs  Project/Timeline/Track/Clip + edit operations
src-tauri/src/ffmpeg.rs ffprobe import, thumbnails, filter_complex export
src-tauri/src/mcp.rs    MCP server (JSON-RPC over HTTP, axum, port 19790)
src-tauri/src/lib.rs    Tauri commands + state + wiring
```

Timeline positions are **frames**; source positions are **seconds** — same
convention as Palmier Pro. Tracks render top-down: index 0 is the topmost
video layer.

## License

GPLv3. Not affiliated with palmier.io — this is an independent
reimplementation inspired by their editor, sharing no code with it.
