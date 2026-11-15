// Mirrors of the Rust types (crates/engine/src/types.rs and src-tauri).

export type Layout = "auto" | "face" | "split" | "fit" | "center";
export type ClipStatus = "pending" | "ready" | "kept" | "exporting" | "exported" | "discarded" | "failed";
export type StageKey = "fetching" | "audio" | "listening" | "picking" | "cutting" | "done";

export interface Word {
  w: string;
  s: number;
  e: number;
}

export interface Pick {
  start: number;
  end: number;
  title: string;
  hook: string;
  score: number;
  reason: string;
}

export interface ClipEdits {
  trim: [number, number] | null;
  deleted: number[];
  crop_x: number | null;
}

export interface Clip {
  id: string;
  index: number;
  pick: Pick;
  section_start: number;
  section_end: number;
  src_w: number;
  src_h: number;
  fps: number;
  words: Word[];
  auto_range: [number, number];
  layout: Layout;
  resolved_layout: Layout;
  caption_style: string;
  hook_banner: boolean;
  edits: ClipEdits;
  status: ClipStatus;
  duration: number;
  preview: string | null;
  thumb: string | null;
  export_path: string | null;
  error: string | null;
  rev: number;
}

export interface SourceMeta {
  input: string;
  is_url: boolean;
  title: string;
  duration: number;
  width: number;
  height: number;
  fps: number;
  uploader: string | null;
  thumbnail: string | null;
  was_live: boolean;
}

export interface Project {
  id: string;
  created: number;
  meta: SourceMeta | null;
  clips: Clip[];
  stage: string;
  done: boolean;
  error: string | null;
}

export interface JobSettings {
  max_clips: number;
  min_len: number;
  max_len: number;
  caption_style: string;
  hook_banner: boolean;
  layout: Layout;
  remove_silence: boolean;
  content: string;
}

export interface AiConfig {
  provider: string;
  base_url: string;
  model: string;
  stt: "cloud" | "local";
  whisper_size: string;
  stt_provider: string | null;
}

export interface Settings {
  onboarded: boolean;
  ai: AiConfig | null;
  job: JobSettings;
  export_dir: string;
  launch_at_login: boolean;
  listen_budget_min: number;
}

export interface Preset {
  id: string;
  name: string;
  tagline: string;
  base_url: string;
  key_url: string;
  needs_key: boolean;
  stt_model: string | null;
  free: boolean;
}

export interface CaptionStyle {
  id: string;
  name: string;
  font: string;
  size: number;
  color: string;
  highlight: string;
  outline: string;
  outline_w: number;
  shadow: number;
  upper: boolean;
  max_words: number;
  max_chars: number;
  anim: string;
}

export interface DepsStatus {
  ffmpeg: boolean;
  ytdlp: boolean;
  deno: boolean;
  whisper: boolean;
  whisper_models: string[];
}

export interface AppInfo {
  settings: Settings;
  has_key: boolean;
  deps: DepsStatus;
  presets: Preset[];
  styles: CaptionStyle[];
  platform: "windows" | "macos";
  version: string;
  data_dir: string;
}

export interface DepProgress {
  item: string;
  label: string;
  done: number;
  total: number;
  finished: boolean;
}

export interface QueueState {
  current: string | null;
  pending: string[];
  exporting: [string, string] | null;
  export_pending: [string, string][];
}

export type EngineEvent =
  | { kind: "progress"; project_id: string; stage: StageKey; label: string; progress: number; detail: string }
  | { kind: "project"; project: Project }
  | { kind: "clip"; project_id: string; clip: Clip }
  | { kind: "notice"; project_id: string; message: string }
  | { kind: "done"; project_id: string }
  | { kind: "failed"; project_id: string; error: string };

export interface ExportProgress {
  project_id: string;
  clip_id: string;
  progress: number;
}

export interface ClipPatch {
  layout?: Layout;
  caption_style?: string;
  hook_banner?: boolean;
  hook?: string;
  title?: string;
  trim?: [number, number] | null;
  deleted?: number[];
  crop_x?: number | null;
}
