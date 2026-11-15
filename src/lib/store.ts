import { create } from "zustand";
import { api, on } from "./api";
import type { AppInfo, Clip, EngineEvent, Project, QueueState, StageKey } from "./types";

export interface JobProgress {
  stage: StageKey;
  label: string;
  progress: number;
  detail: string;
}

export interface Toast {
  id: number;
  text: string;
  tone: "info" | "error" | "good";
}

interface State {
  info: AppInfo | null;
  projects: Record<string, Project>;
  progress: Record<string, JobProgress>;
  queue: QueueState;
  exportProgress: Record<string, number>;
  toasts: Toast[];
  loadInfo: () => Promise<AppInfo>;
  loadProjects: () => Promise<void>;
  upsertClip: (projectId: string, clip: Clip) => void;
  toast: (text: string, tone?: Toast["tone"]) => void;
  dropToast: (id: number) => void;
}

let toastId = 1;

export const useStore = create<State>((set, get) => ({
  info: null,
  projects: {},
  progress: {},
  queue: { current: null, pending: [], exporting: null, export_pending: [] },
  exportProgress: {},
  toasts: [],
  loadInfo: async () => {
    const info = await api.appInfo();
    set({ info });
    return info;
  },
  loadProjects: async () => {
    const list = await api.listProjects();
    const projects: Record<string, Project> = {};
    for (const p of list) projects[p.id] = p;
    set({ projects });
  },
  upsertClip: (projectId, clip) => {
    const p = get().projects[projectId];
    if (!p) return;
    const clips = p.clips.filter((c) => c.id !== clip.id).concat(clip).sort((a, b) => a.index - b.index);
    set({ projects: { ...get().projects, [projectId]: { ...p, clips } } });
  },
  toast: (text, tone = "info") => {
    const id = toastId++;
    set({ toasts: [...get().toasts, { id, text, tone }] });
    setTimeout(() => get().dropToast(id), tone === "error" ? 7000 : 4200);
  },
  dropToast: (id) => set({ toasts: get().toasts.filter((t) => t.id !== id) }),
}));

function handle(e: EngineEvent) {
  const s = useStore.getState();
  switch (e.kind) {
    case "progress":
      useStore.setState({
        progress: { ...s.progress, [e.project_id]: { stage: e.stage, label: e.label, progress: e.progress, detail: e.detail } },
      });
      break;
    case "project":
      useStore.setState({ projects: { ...s.projects, [e.project.id]: e.project } });
      break;
    case "clip":
      s.upsertClip(e.project_id, e.clip);
      break;
    case "notice":
      s.toast(e.message, "info");
      break;
    case "done": {
      const { [e.project_id]: _, ...rest } = s.progress;
      useStore.setState({ progress: rest });
      break;
    }
    case "failed": {
      const { [e.project_id]: _, ...rest } = s.progress;
      const p = s.projects[e.project_id];
      useStore.setState({
        progress: rest,
        projects: p ? { ...s.projects, [e.project_id]: { ...p, error: e.error } } : s.projects,
      });
      if (e.error !== "Cancelled") s.toast(e.error, "error");
      break;
    }
  }
}

let wired = false;

/** Subscribe this window to backend events (once). */
export async function wireEvents() {
  if (wired) return;
  wired = true;
  await on.engine(handle);
  await on.queue((queue) => useStore.setState({ queue }));
  await on.exportProgress((p) => {
    const s = useStore.getState();
    useStore.setState({ exportProgress: { ...s.exportProgress, [`${p.project_id}/${p.clip_id}`]: p.progress } });
  });
  useStore.setState({ queue: await api.queueState() });
}
