export type MediaKind = "video" | "audio" | "image";
export type TrackKind = "video" | "audio";

export interface MediaAsset {
  id: string;
  path: string;
  name: string;
  kind: MediaKind;
  duration: number;
  width: number;
  height: number;
  fps: number;
  has_audio: boolean;
  thumbnail: string | null;
}

export interface Clip {
  id: string;
  media_id: string;
  start_frame: number;
  end_frame: number;
  source_in: number;
  volume: number;
  opacity: number;
}

export interface Track {
  id: string;
  kind: TrackKind;
  name: string;
  clips: Clip[];
}

export interface Timeline {
  fps: number;
  width: number;
  height: number;
  tracks: Track[];
}

export interface Project {
  name: string;
  path: string | null;
  media: MediaAsset[];
  timeline: Timeline;
}

export interface ExportProgress {
  percent: number;
  done: boolean;
  error: string | null;
}
