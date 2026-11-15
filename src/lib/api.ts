import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AiConfig,
  AppInfo,
  Clip,
  ClipPatch,
  DepProgress,
  DepsStatus,
  EngineEvent,
  ExportProgress,
  Project,
  QueueState,
  Settings,
} from "./types";

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  installDeps: () => invoke<DepsStatus>("install_deps"),
  installWhisper: (size: string) => invoke<DepsStatus>("install_whisper", { size }),
  testAi: (provider: string, baseUrl: string | null, key: string | null) =>
    invoke<{ models: string[]; chosen: string | null }>("test_ai", { provider, baseUrl, key }),
  saveAi: (config: AiConfig, key: string | null, sttKey: string | null) => invoke<void>("save_ai", { config, key, sttKey }),
  forgetKey: (provider: string) => invoke<void>("forget_key", { provider }),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  finishOnboarding: () => invoke<void>("finish_onboarding"),
  startJob: (input: string) => invoke<string>("start_job", { input }),
  cancelJob: (projectId: string) => invoke<void>("cancel_job", { projectId }),
  queueState: () => invoke<QueueState>("queue_state"),
  listProjects: () => invoke<Project[]>("list_projects"),
  deleteProject: (projectId: string) => invoke<void>("delete_project", { projectId }),
  keepClip: (projectId: string, clipId: string) => invoke<Clip>("keep_clip", { projectId, clipId }),
  discardClip: (projectId: string, clipId: string) => invoke<Clip>("discard_clip", { projectId, clipId }),
  restoreClip: (projectId: string, clipId: string) => invoke<Clip>("restore_clip", { projectId, clipId }),
  editClip: (projectId: string, clipId: string, patch: ClipPatch) => invoke<Clip>("edit_clip", { projectId, clipId, patch }),
  exportClip: (projectId: string, clipId: string) => invoke<void>("export_clip", { projectId, clipId }),
  setHitRects: (rects: Rect[], dropZone: Rect | null) => invoke<void>("set_hit_rects", { rects, dropZone }),
  focusTray: () => invoke<void>("focus_tray"),
  dragTray: () => invoke<void>("drag_tray"),
  setTrayCorner: (corner: string) => invoke<void>("set_tray_corner", { corner }),
  openStudio: (view?: string) => invoke<void>("open_studio", { view: view ?? null }),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  quit: () => invoke<void>("quit"),
};

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export const on = {
  engine: (f: (e: EngineEvent) => void): Promise<UnlistenFn> => listen<EngineEvent>("cd://event", (e) => f(e.payload)),
  deps: (f: (p: DepProgress) => void) => listen<DepProgress>("cd://deps", (e) => f(e.payload)),
  queue: (f: (q: QueueState) => void) => listen<QueueState>("cd://queue", (e) => f(e.payload)),
  exportProgress: (f: (p: ExportProgress) => void) => listen<ExportProgress>("cd://export", (e) => f(e.payload)),
  dragNear: (f: (near: boolean) => void) => listen<boolean>("cd://drag-near", (e) => f(e.payload)),
  openLink: (f: () => void) => listen("cd://open-link", () => f()),
  navigate: (f: (view: string) => void) => listen<string>("cd://navigate", (e) => f(e.payload)),
};

/** URL for a local file (preview video, thumbnail), cache-busted by revision. */
export function fileUrl(path: string | null | undefined, rev = 0): string | undefined {
  if (!path) return undefined;
  return `${convertFileSrc(path)}?v=${rev}`;
}

export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}

export function fmtDur(s: number): string {
  if (!isFinite(s) || s <= 0) return "0:00";
  const t = Math.round(s);
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const sec = t % 60;
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}` : `${m}:${String(sec).padStart(2, "0")}`;
}

export function isLink(s: string): boolean {
  return /^https?:\/\/\S+$/i.test(s.trim());
}

export function linkHost(s: string): string {
  try {
    const h = new URL(s.trim()).hostname.replace(/^www\./, "");
    if (h.includes("youtu")) return "YouTube";
    if (h.includes("kick")) return "Kick";
    if (h.includes("twitch")) return "Twitch";
    return h;
  } catch {
    return "Link";
  }
}
