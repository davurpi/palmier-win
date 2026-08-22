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

## Features (v0.1)

- Project management (`.palmierwin` JSON projects), New/Open/Save
- Import video, audio and images (ffprobe metadata + thumbnails)
- Multi-track timeline: drag, move across tracks, trim, razor split (S),
  delete (Del), zoom (Ctrl+wheel), snapping playhead ruler
- Clip properties: opacity (video), volume (audio)
- Composited preview player (track 0 on top, like the original)
- Export to H.264 / H.265 MP4 with AAC audio via FFmpeg `filter_complex`
  (black canvas → scale/pad → overlay chain → amix), with live progress
- **MCP server** with 11 tools: `get_project`, `import_media`, `add_track`,
  `add_clip`, `move_clip`, `trim_clip`, `split_clip`, `remove_clips`,
  `set_clip_properties`, `set_timeline_settings`, `export_project`

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
