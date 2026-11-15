import { AnimatePresence, motion } from "motion/react";
import { ArrowRight, FolderOpen, Gamepad2, Link2, Loader2, Mic, Sparkles, Trash2, Upload, Video } from "lucide-react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { Button, Ring } from "../components/ui";
import { api, errText, fileUrl, fmtDur, isLink, linkHost } from "../lib/api";
import { useStore } from "../lib/store";
import type { Project } from "../lib/types";

const VIDEO_EXT = ["mp4", "mov", "mkv", "webm", "m4v", "avi", "flv", "ts"];

export function useStartJob() {
  const toast = useStore((s) => s.toast);
  return async (input: string): Promise<string | null> => {
    try {
      return await api.startJob(input);
    } catch (e) {
      toast(errText(e), "error");
      return null;
    }
  };
}

const CONTENT = [
  { id: "auto", label: "Anything", icon: Sparkles },
  { id: "gaming", label: "Gaming & streams", icon: Gamepad2 },
  { id: "podcast", label: "Podcast", icon: Mic },
  { id: "vlog", label: "Vlog & challenges", icon: Video },
];

export function Library({ open }: { open: (id: string) => void }) {
  const projects = useStore((s) => s.projects);
  const info = useStore((s) => s.info);
  const loadInfo = useStore((s) => s.loadInfo);
  const start = useStartJob();
  const [link, setLink] = useState("");
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const list = Object.values(projects).sort((a, b) => b.created - a.created);
  const content = info?.settings.job.content ?? "auto";

  async function go(input: string) {
    setBusy(true);
    const id = await start(input);
    setBusy(false);
    if (id) {
      setLink("");
      open(id);
    }
  }

  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((e) => {
      const t = e.payload.type;
      if (t === "enter" || t === "over") setDragging(true);
      else if (t === "leave") setDragging(false);
      else if (t === "drop") {
        setDragging(false);
        const f = e.payload.paths.find((p) => VIDEO_EXT.includes(p.split(".").pop()?.toLowerCase() ?? ""));
        if (f) go(f);
      }
    });
    return () => {
      un.then((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function browse() {
    const f = await openDialog({ multiple: false, filters: [{ name: "Video", extensions: VIDEO_EXT }] });
    if (typeof f === "string") go(f);
  }

  async function setContent(id: string) {
    if (!info) return;
    await api.saveSettings({ ...info.settings, job: { ...info.settings.job, content: id } });
    await loadInfo();
  }

  const valid = isLink(link);

  return (
    <div className="scroll-thin h-full overflow-y-auto">
      <div className="mx-auto max-w-[1080px] px-10 pb-16 pt-10">
        <motion.div initial={{ opacity: 0, y: 14 }} animate={{ opacity: 1, y: 0 }} transition={{ type: "spring", stiffness: 200, damping: 24 }}>
          <h1 className="font-display text-[52px] font-bold leading-[1] tracking-[-0.045em]">
            What are we <span className="text-gradient">clipping</span> today?
          </h1>
          <p className="mt-3 text-[15px] text-mute">A YouTube, Kick or Twitch link, or any video file. Long streams are fine.</p>
        </motion.div>

        <motion.div
          initial={{ opacity: 0, y: 18 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ type: "spring", stiffness: 200, damping: 24, delay: 0.05 }}
          className={`relative mt-8 rounded-[28px] p-[1.5px] transition-all ${dragging ? "bg-gradient-to-r from-coral via-amber to-coral" : "bg-gradient-to-r from-white/15 via-white/[0.06] to-white/15"}`}
        >
          <div className="glass-dark relative flex items-center gap-3 rounded-[27px] p-3 pl-6">
            <Link2 size={20} className={valid ? "text-coral" : "text-dim"} />
            <input
              ref={inputRef}
              value={link}
              onChange={(e) => setLink(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && valid && go(link)}
              placeholder="Paste a link…  youtube.com/watch?v=…"
              spellCheck={false}
              className="h-14 flex-1 bg-transparent text-[18px] text-paper placeholder:text-dim outline-none"
            />
            <AnimatePresence>
              {valid && (
                <motion.span initial={{ opacity: 0, scale: 0.8 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.8 }} className="rounded-full bg-white/[0.07] px-3 py-1 text-[12px] font-semibold text-paper/80">
                  {linkHost(link)}
                </motion.span>
              )}
            </AnimatePresence>
            <Button tone="ghost" onClick={browse} title="Choose a video file"><FolderOpen size={17} /> File</Button>
            <Button tone="primary" size="lg" className="h-14 rounded-[20px] px-7" disabled={!valid || busy} onClick={() => go(link)}>
              {busy ? <Loader2 size={18} className="animate-spin" /> : <>Make clips <ArrowRight size={18} /></>}
            </Button>
            <AnimatePresence>
              {dragging && (
                <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} className="absolute inset-0 grid place-items-center rounded-[27px] bg-ink-900/90 text-[17px] font-semibold">
                  <span className="inline-flex items-center gap-2"><Upload size={18} className="text-coral" /> Drop to make clips</span>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </motion.div>

        <div className="mt-4 flex flex-wrap items-center gap-2">
          <span className="mr-1 text-[12.5px] text-dim">Tuned for</span>
          {CONTENT.map((c) => (
            <button
              key={c.id}
              onClick={() => setContent(c.id)}
              className={`inline-flex h-8 items-center gap-1.5 rounded-full border px-3 text-[12.5px] font-medium transition cursor-pointer ${
                content === c.id ? "border-coral/50 bg-coral/10 text-paper" : "border-white/[0.07] text-mute hover:text-paper hover:bg-white/[0.04]"
              }`}
            >
              <c.icon size={13} /> {c.label}
            </button>
          ))}
        </div>

        <div className="mt-14 flex items-end justify-between">
          <h2 className="font-display text-[22px] font-semibold tracking-tight">Your videos</h2>
          <span className="text-[12.5px] text-dim">{list.length} total</span>
        </div>
        {list.length === 0 ? (
          <EmptyState />
        ) : (
          <div className="mt-5 grid grid-cols-2 gap-5 xl:grid-cols-3">
            {list.map((p, i) => (
              <ProjectCard key={p.id} p={p} i={i} open={() => open(p.id)} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function EmptyState() {
  return (
    <div className="mt-5 grid h-48 place-items-center rounded-3xl border border-dashed border-white/10 text-center">
      <div>
        <div className="font-display text-[18px] font-semibold">No videos yet</div>
        <div className="mt-1 text-[13px] text-mute">Paste a link above, or drop a video anywhere on this window.</div>
      </div>
    </div>
  );
}

function ProjectCard({ p, i, open }: { p: Project; i: number; open: () => void }) {
  const prog = useStore((s) => s.progress[p.id]);
  const queue = useStore((s) => s.queue);
  const queued = queue.pending.includes(p.id);
  const clips = p.clips.filter((c) => c.status !== "discarded");
  const cover = p.meta?.thumbnail ?? fileUrl(clips[0]?.thumb, clips[0]?.rev);
  const toast = useStore((s) => s.toast);
  const loadProjects = useStore((s) => s.loadProjects);
  async function remove(e: React.MouseEvent) {
    e.stopPropagation();
    try {
      await api.deleteProject(p.id);
      await loadProjects();
    } catch (err) {
      toast(errText(err), "error");
    }
  }
  return (
    <motion.button
      onClick={open}
      initial={{ opacity: 0, y: 16 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: Math.min(i, 8) * 0.04, type: "spring", stiffness: 220, damping: 24 }}
      whileHover={{ y: -4 }}
      className="group relative overflow-hidden rounded-3xl border border-white/[0.07] bg-white/[0.025] text-left cursor-pointer"
    >
      <div className="relative aspect-video overflow-hidden bg-ink-800">
        {cover && <img src={cover} className="h-full w-full object-cover opacity-80 transition duration-500 group-hover:scale-105 group-hover:opacity-100" />}
        <div className="absolute inset-0 bg-gradient-to-t from-ink-950 via-ink-950/20 to-transparent" />
        <div className="absolute bottom-3 left-3 flex -space-x-3">
          {clips.slice(0, 5).map((c, j) => (
            <div key={c.id} className="h-14 w-8 overflow-hidden rounded-md border border-black/50 bg-ink-700 shadow-lg" style={{ transform: `rotate(${(j - 2) * 4}deg)` }}>
              {c.thumb && <img src={fileUrl(c.thumb, c.rev)} className="h-full w-full object-cover" />}
            </div>
          ))}
        </div>
        {(prog || queued) && (
          <div className="absolute right-3 top-3 grid place-items-center">
            <Ring value={prog?.progress ?? 0} size={44} stroke={3.5} />
            <span className="absolute font-mono text-[10px]">{prog ? Math.round(prog.progress * 100) : "…"}</span>
          </div>
        )}
        {p.meta && <span className="absolute bottom-3 right-3 rounded-md bg-black/60 px-1.5 py-0.5 font-mono text-[11px]">{fmtDur(p.meta.duration)}</span>}
      </div>
      <div className="px-4 pb-4 pt-3">
        <div className="truncate text-[14.5px] font-semibold">{p.meta?.title ?? "Opening…"}</div>
        <div className="mt-0.5 truncate text-[12.5px] text-mute">
          {prog ? `${prog.label} · ${prog.detail}` : queued ? "Waiting in line" : p.error ? <span className="text-coral-soft">{p.error}</span> : `${clips.length} clips${p.meta?.uploader ? ` · ${p.meta.uploader}` : ""}`}
        </div>
      </div>
      <span onClick={remove} className="absolute right-3 top-3 hidden rounded-lg bg-black/60 p-1.5 text-mute hover:text-coral group-hover:block" title="Delete">
        {!prog && <Trash2 size={14} />}
      </span>
    </motion.button>
  );
}
