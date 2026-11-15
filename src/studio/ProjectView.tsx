import { AnimatePresence, motion } from "motion/react";
import {
  ArrowLeft,
  Check,
  Columns2,
  ExternalLink,
  FolderSearch,
  Loader2,
  Maximize,
  Pause,
  Play,
  RotateCcw,
  ScanFace,
  Sparkles,
  Square,
  Undo2,
  Wand2,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CaptionSample } from "../components/CaptionSample";
import { Button, Ring, ScoreDot, Segmented, Toggle } from "../components/ui";
import { api, errText, fileUrl, fmtDur } from "../lib/api";
import { useStore } from "../lib/store";
import type { Clip, ClipPatch, Layout, Project } from "../lib/types";
import { useStartJob } from "./Library";

const STAGES = [
  { key: "fetching", label: "Fetch" },
  { key: "audio", label: "Audio" },
  { key: "listening", label: "Listen" },
  { key: "picking", label: "Pick" },
  { key: "cutting", label: "Cut" },
];

export function ProjectView({ id, back }: { id: string; back: () => void }) {
  const p = useStore((s) => s.projects[id]);
  const prog = useStore((s) => s.progress[id]);
  const queue = useStore((s) => s.queue);
  const [sel, setSel] = useState<string | null>(null);
  const clips = useMemo(() => (p?.clips ?? []).slice().sort((a, b) => a.index - b.index), [p]);
  const selected = clips.find((c) => c.id === sel) ?? clips.find((c) => c.status !== "discarded") ?? null;
  const running = queue.current === id || queue.pending.includes(id) || !!prog;

  if (!p) {
    return (
      <div className="grid h-full place-items-center text-mute">
        <div className="text-center">
          <div>This video is gone.</div>
          <Button className="mt-4" onClick={back}><ArrowLeft size={15} /> Back</Button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <Header p={p} running={running} back={back} />
      {clips.length === 0 ? (
        <Waiting p={p} running={running} />
      ) : (
        <div className="flex min-h-0 flex-1">
          <ClipList pid={id} clips={clips} selected={selected?.id ?? null} onSelect={setSel} running={running} />
          {selected && <Editor key={selected.id} p={p} clip={selected} />}
        </div>
      )}
    </div>
  );
}

function Header({ p, running, back }: { p: Project; running: boolean; back: () => void }) {
  const prog = useStore((s) => s.progress[p.id]);
  const start = useStartJob();
  const stageIdx = STAGES.findIndex((s) => s.key === (prog?.stage ?? p.stage));
  return (
    <div className="flex shrink-0 items-center gap-4 border-b hairline px-6 py-4">
      <button onClick={back} className="grid h-9 w-9 place-items-center rounded-xl text-mute hover:bg-white/[0.06] hover:text-paper cursor-pointer"><ArrowLeft size={17} /></button>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <h1 className="truncate font-display text-[21px] font-semibold tracking-tight">{p.meta?.title ?? "Opening…"}</h1>
          {p.meta?.is_url && (
            <button className="text-dim hover:text-paper cursor-pointer" onClick={() => openUrl(p.meta!.input)} title="Open the original">
              <ExternalLink size={14} />
            </button>
          )}
        </div>
        <div className="mt-0.5 flex items-center gap-2 text-[12.5px] text-mute">
          {p.meta?.uploader && <span>{p.meta.uploader}</span>}
          {p.meta && <span className="font-mono">{fmtDur(p.meta.duration)}</span>}
          {p.error && !running && <span className="truncate text-coral-soft">· {p.error}</span>}
        </div>
      </div>
      {running && (
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-1.5">
            {STAGES.map((s, i) => (
              <div key={s.key} className="flex flex-col items-center gap-1">
                <div className="relative h-1 w-12 overflow-hidden rounded-full bg-white/10">
                  <motion.div
                    className="absolute inset-y-0 left-0 rounded-full bg-gradient-to-r from-amber to-coral"
                    animate={{ width: i < stageIdx ? "100%" : i === stageIdx ? `${(prog?.progress ?? 0) * 100}%` : "0%" }}
                  />
                </div>
                <span className={`text-[10.5px] ${i === stageIdx ? "text-paper" : "text-dim"}`}>{s.label}</span>
              </div>
            ))}
          </div>
          <Button size="sm" tone="ghost" onClick={() => api.cancelJob(p.id)}><Square size={12} /> Stop</Button>
        </div>
      )}
      {!running && (p.error || !p.done) && p.meta && (
        <Button size="sm" tone="soft" onClick={() => start(p.meta!.input)}><RotateCcw size={13} /> {p.error ? "Try again" : "Resume"}</Button>
      )}
    </div>
  );
}

function Waiting({ p, running }: { p: Project; running: boolean }) {
  const prog = useStore((s) => s.progress[p.id]);
  return (
    <div className="grid flex-1 place-items-center">
      <div className="flex flex-col items-center text-center">
        <div className="relative h-[220px] w-[260px]">
          {[0, 1, 2, 3].map((i) => (
            <motion.div
              key={i}
              className="developing absolute left-1/2 top-1/2 h-[180px] w-[101px] rounded-2xl border border-white/10 bg-gradient-to-b from-ink-700 to-ink-850"
              style={{ marginLeft: -50, marginTop: -90 }}
              animate={running ? { rotate: [(i - 1.5) * 8, (i - 1.5) * 12, (i - 1.5) * 8], x: (i - 1.5) * 36, y: [0, -6, 0] } : { rotate: (i - 1.5) * 6, x: (i - 1.5) * 26 }}
              transition={{ duration: 2.4, repeat: Infinity, delay: i * 0.15, ease: "easeInOut" }}
            />
          ))}
        </div>
        <div className="mt-4 font-display text-[22px] font-semibold">{running ? prog?.label ?? "Waiting in line" : p.error ? "Stopped" : "No clips"}</div>
        <div className="mt-1 max-w-[420px] text-[13.5px] text-mute">
          {running ? prog?.detail ?? "Another video is being cut first." : p.error ?? "Cutdeck didn't find moments in this video."}
        </div>
        {running && prog && <div className="mt-4 font-mono text-[13px] text-amber">{Math.round(prog.progress * 100)}%</div>}
      </div>
    </div>
  );
}

function statusChip(c: Clip, exportProg?: number) {
  switch (c.status) {
    case "exported":
      return <span className="text-mint">Exported</span>;
    case "exporting":
    case "kept":
      return <span className="text-amber">{exportProg !== undefined ? `Exporting ${Math.round(exportProg * 100)}%` : "Queued"}</span>;
    case "discarded":
      return <span className="text-dim">Skipped</span>;
    default:
      return null;
  }
}

function ClipList({ pid, clips, selected, onSelect, running }: { pid: string; clips: Clip[]; selected: string | null; onSelect: (id: string) => void; running: boolean }) {
  const exp = useStore((s) => s.exportProgress);
  return (
    <div className="scroll-thin w-[300px] shrink-0 overflow-y-auto border-r hairline p-3">
      <div className="mb-2 flex items-center justify-between px-1 text-[11px] font-semibold uppercase tracking-[0.12em] text-dim">
        <span>{clips.length} clips</span>
        {running && <span className="inline-flex items-center gap-1 normal-case tracking-normal text-amber"><Loader2 size={11} className="animate-spin" /> more coming</span>}
      </div>
      <div className="flex flex-col gap-1.5">
        <AnimatePresence initial={false}>
          {clips.map((c) => {
            const active = c.id === selected;
            const ep = exp[`${pid}/${c.id}`];
            return (
              <motion.button
                key={c.id}
                layout
                initial={{ opacity: 0, x: -20, scale: 0.95 }}
                animate={{ opacity: c.status === "discarded" ? 0.45 : 1, x: 0, scale: 1 }}
                onClick={() => onSelect(c.id)}
                className={`flex gap-3 rounded-2xl p-2 text-left transition-colors cursor-pointer ${active ? "bg-white/[0.08] ring-1 ring-coral/40" : "hover:bg-white/[0.04]"}`}
              >
                <div className="relative h-[92px] w-[52px] shrink-0 overflow-hidden rounded-lg bg-ink-700">
                  {c.thumb && <img src={fileUrl(c.thumb, c.rev)} className="h-full w-full object-cover" />}
                </div>
                <div className="min-w-0 flex-1 py-0.5">
                  <div className="line-clamp-2 text-[13px] font-medium leading-snug">{c.pick.title}</div>
                  <div className="mt-1.5 flex items-center gap-2 text-[11.5px] text-mute">
                    <ScoreDot score={c.pick.score} />
                    <span className="font-mono">{fmtDur(c.duration)}</span>
                  </div>
                  <div className="mt-1 text-[11.5px]">{statusChip(c, ep)}</div>
                </div>
              </motion.button>
            );
          })}
        </AnimatePresence>
      </div>
    </div>
  );
}

const LAYOUTS: { value: Layout; label: React.ReactNode; title: string }[] = [
  { value: "auto", label: <Wand2 size={14} className="mx-auto" />, title: "Auto" },
  { value: "face", label: <ScanFace size={14} className="mx-auto" />, title: "Follow face" },
  { value: "split", label: <Columns2 size={14} className="mx-auto rotate-90" />, title: "Facecam + gameplay" },
  { value: "fit", label: <Maximize size={14} className="mx-auto" />, title: "Whole frame" },
  { value: "center", label: <Square size={13} className="mx-auto" />, title: "Center crop" },
];

const LAYOUT_NAMES: Record<Layout, string> = { auto: "Auto", face: "Follows the speaker", split: "Facecam on top, gameplay below", fit: "Whole frame on a blurred background", center: "Center crop" };

function Editor({ p, clip }: { p: Project; clip: Clip }) {
  const styles = useStore((s) => s.info?.styles ?? []);
  const toast = useStore((s) => s.toast);
  const exportProg = useStore((s) => s.exportProgress[`${p.id}/${clip.id}`]);
  const [busy, setBusy] = useState(false);
  const [tab, setTab] = useState<"look" | "words">("look");
  const [hook, setHook] = useState(clip.pick.hook);
  const [title, setTitle] = useState(clip.pick.title);

  async function patch(pt: ClipPatch) {
    setBusy(true);
    try {
      await api.editClip(p.id, clip.id, pt);
    } catch (e) {
      toast(errText(e), "error");
    } finally {
      setBusy(false);
    }
  }

  const keep = async () => {
    try {
      await api.keepClip(p.id, clip.id);
      useStore.getState().upsertClip(p.id, { ...clip, status: "kept" });
      toast("Exporting in 1080×1920…", "good");
    } catch (e) {
      toast(errText(e), "error");
    }
  };
  const skip = async () => {
    const c = clip.status === "discarded" ? await api.restoreClip(p.id, clip.id) : await api.discardClip(p.id, clip.id);
    useStore.getState().upsertClip(p.id, c);
  };

  return (
    <div className="flex min-w-0 flex-1">
      <div className="relative flex min-w-0 flex-1 flex-col items-center justify-center gap-5 p-6">
        <Player clip={clip} busy={busy} />
        <div className="flex items-center gap-2">
          <Button tone="ghost" onClick={skip}>
            {clip.status === "discarded" ? <><Undo2 size={15} /> Restore</> : <><X size={15} /> Skip</>}
          </Button>
          {clip.status === "exported" && clip.export_path ? (
            <>
              <Button tone="soft" onClick={() => api.reveal(clip.export_path!)}><FolderSearch size={15} /> Show file</Button>
              <Button tone="ghost" onClick={() => api.exportClip(p.id, clip.id)}><RotateCcw size={14} /> Export again</Button>
            </>
          ) : clip.status === "kept" || clip.status === "exporting" ? (
            <Button tone="soft" disabled className="w-48">
              <Ring value={exportProg ?? 0} size={18} stroke={2.5} /> {exportProg !== undefined ? `Exporting ${Math.round(exportProg * 100)}%` : "Waiting to export"}
            </Button>
          ) : (
            <Button tone="primary" onClick={keep} disabled={clip.status === "discarded"} className="px-6">
              <Check size={16} /> Keep and export
            </Button>
          )}
        </div>
      </div>

      <div className="flex w-[360px] shrink-0 flex-col border-l hairline">
        <div className="flex gap-1 border-b hairline p-2">
          {(["look", "words"] as const).map((t) => (
            <button key={t} onClick={() => setTab(t)} className={`relative h-9 flex-1 rounded-lg text-[13px] font-medium cursor-pointer ${tab === t ? "text-paper" : "text-mute hover:text-paper"}`}>
              {tab === t && <motion.span layoutId="ed-tab" className="absolute inset-0 -z-0 rounded-lg bg-white/[0.07]" />}
              <span className="relative">{t === "look" ? "Look" : "Words"}</span>
            </button>
          ))}
        </div>
        <div className="scroll-thin flex-1 overflow-y-auto p-5">
          {tab === "look" ? (
            <div className="flex flex-col gap-6">
              <section>
                <div className="mb-2 flex items-baseline justify-between">
                  <span className="text-[12px] font-semibold uppercase tracking-[0.1em] text-dim">Why this moment</span>
                  <ScoreDot score={clip.pick.score} />
                </div>
                <p className="text-[13px] leading-relaxed text-paper/80">{clip.pick.reason || "High energy and a clear hook."}</p>
              </section>
              <section>
                <div className="mb-2 text-[12px] font-semibold uppercase tracking-[0.1em] text-dim">Framing</div>
                <Segmented value={clip.layout} onChange={(l) => patch({ layout: l })} options={LAYOUTS} disabled={busy} />
                <div className="mt-2 text-[12px] text-mute">
                  {clip.layout === "auto" ? `Auto chose: ${LAYOUT_NAMES[clip.resolved_layout].toLowerCase()}` : LAYOUT_NAMES[clip.layout]}
                </div>
                {(clip.resolved_layout === "face" || clip.resolved_layout === "center") && (
                  <div className="mt-3">
                    <div className="mb-1 flex justify-between text-[11.5px] text-dim"><span>Left</span><span>Crop position</span><span>Right</span></div>
                    <input
                      type="range"
                      min={0}
                      max={1}
                      step={0.01}
                      defaultValue={clip.edits.crop_x ?? 0.5}
                      onPointerUp={(e) => patch({ crop_x: Number((e.target as HTMLInputElement).value) })}
                      className="w-full accent-[#FF5A4E]"
                    />
                    {clip.edits.crop_x !== null && (
                      <button className="mt-1 text-[12px] text-coral-soft hover:text-coral cursor-pointer" onClick={() => patch({ crop_x: null })}>Back to automatic tracking</button>
                    )}
                  </div>
                )}
              </section>
              <section>
                <div className="mb-2 text-[12px] font-semibold uppercase tracking-[0.1em] text-dim">Captions</div>
                <div className="grid grid-cols-2 gap-2">
                  {styles.map((s) => (
                    <button
                      key={s.id}
                      disabled={busy}
                      onClick={() => patch({ caption_style: s.id })}
                      className={`relative flex h-[74px] flex-col items-center justify-center gap-1.5 overflow-hidden rounded-xl border transition cursor-pointer ${
                        clip.caption_style === s.id ? "border-coral/60 bg-coral/[0.08]" : "border-white/[0.07] bg-black/30 hover:bg-white/[0.04]"
                      }`}
                    >
                      <CaptionSample style={s} scale={0.24} words={["SO", "GOOD"]} active={1} />
                      <span className="text-[10.5px] text-mute">{s.name}</span>
                    </button>
                  ))}
                </div>
              </section>
              <section>
                <div className="mb-2 flex items-center justify-between">
                  <span className="text-[12px] font-semibold uppercase tracking-[0.1em] text-dim">Hook banner</span>
                  <Toggle on={clip.hook_banner} onChange={(v) => patch({ hook_banner: v })} />
                </div>
                <input
                  value={hook}
                  onChange={(e) => setHook(e.target.value)}
                  onBlur={() => hook !== clip.pick.hook && patch({ hook })}
                  onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
                  placeholder="Shown for the first 3 seconds"
                  className="h-10 w-full rounded-xl border border-white/[0.08] bg-black/30 px-3 text-[13.5px] outline-none focus:border-coral/50"
                />
              </section>
              <section>
                <div className="mb-2 text-[12px] font-semibold uppercase tracking-[0.1em] text-dim">Title (file name)</div>
                <input
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  onBlur={() => title !== clip.pick.title && patch({ title })}
                  onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
                  className="h-10 w-full rounded-xl border border-white/[0.08] bg-black/30 px-3 text-[13.5px] outline-none focus:border-coral/50"
                />
              </section>
            </div>
          ) : (
            <Words clip={clip} busy={busy} patch={patch} />
          )}
        </div>
      </div>
    </div>
  );
}

function Player({ clip, busy }: { clip: Clip; busy: boolean }) {
  const ref = useRef<HTMLVideoElement>(null);
  const [playing, setPlaying] = useState(false);
  const [t, setT] = useState(0);
  const src = fileUrl(clip.preview, clip.rev);
  useEffect(() => {
    setPlaying(false);
    setT(0);
  }, [src]);
  const toggle = () => {
    const v = ref.current;
    if (!v) return;
    if (v.paused) {
      v.play();
      setPlaying(true);
    } else {
      v.pause();
      setPlaying(false);
    }
  };
  return (
    <div className="relative">
      <div className="absolute -inset-10 rounded-[60px] bg-coral/10 blur-3xl" />
      <motion.div
        layout
        className={`relative aspect-[9/16] h-[min(66vh,620px)] overflow-hidden rounded-[28px] border border-white/10 bg-black shadow-[0_40px_100px_-30px_rgba(0,0,0,0.95)] ${busy ? "developing" : ""}`}
      >
        <video
          ref={ref}
          src={src}
          poster={fileUrl(clip.thumb, clip.rev)}
          className="h-full w-full object-cover"
          loop
          playsInline
          onClick={toggle}
          onTimeUpdate={(e) => setT((e.target as HTMLVideoElement).currentTime)}
        />
        <AnimatePresence>
          {!playing && !busy && (
            <motion.button
              initial={{ opacity: 0, scale: 0.8 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 1.2 }}
              onClick={toggle}
              className="absolute inset-0 m-auto grid h-16 w-16 place-items-center rounded-full bg-white/15 backdrop-blur-md cursor-pointer"
            >
              <Play size={26} fill="white" className="ml-1" />
            </motion.button>
          )}
        </AnimatePresence>
        {busy && (
          <div className="absolute inset-x-0 bottom-16 z-10 flex justify-center">
            <span className="inline-flex items-center gap-2 rounded-full bg-black/60 px-3 py-1.5 text-[12px] backdrop-blur"><Sparkles size={13} className="text-amber" /> Developing…</span>
          </div>
        )}
        <div className="absolute inset-x-4 bottom-4 flex items-center gap-3">
          <button onClick={toggle} className="text-white/90 cursor-pointer">{playing ? <Pause size={15} fill="white" /> : <Play size={15} fill="white" />}</button>
          <div
            className="relative h-1 flex-1 cursor-pointer rounded-full bg-white/20"
            onClick={(e) => {
              const r = (e.currentTarget as HTMLDivElement).getBoundingClientRect();
              if (ref.current) ref.current.currentTime = ((e.clientX - r.left) / r.width) * clip.duration;
            }}
          >
            <div className="absolute inset-y-0 left-0 rounded-full bg-white" style={{ width: `${(t / Math.max(0.1, clip.duration)) * 100}%` }} />
          </div>
          <span className="font-mono text-[11px] text-white/80">{fmtDur(t)} / {fmtDur(clip.duration)}</span>
        </div>
      </motion.div>
    </div>
  );
}

function Words({ clip, busy, patch }: { clip: Clip; busy: boolean; patch: (p: ClipPatch) => Promise<void> }) {
  const range = clip.edits.trim ?? clip.auto_range;
  const [start, setStart] = useState(range[0]);
  const [end, setEnd] = useState(range[1]);
  const [deleted, setDeleted] = useState<Set<number>>(new Set(clip.edits.deleted));
  const [hover, setHover] = useState<number | null>(null);
  const dirty = start !== range[0] || end !== range[1] || deleted.size !== clip.edits.deleted.length || [...deleted].some((d) => !clip.edits.deleted.includes(d));
  const toggle = (i: number) => {
    const n = new Set(deleted);
    if (n.has(i)) n.delete(i);
    else n.add(i);
    setDeleted(n);
  };
  const dur = clip.words[end] && clip.words[start] ? clip.words[end].e - clip.words[start].s : 0;
  return (
    <div className="flex flex-col gap-4">
      <p className="text-[12.5px] leading-relaxed text-mute">
        Click a word to cut it. Hover a word to start or end the clip there. Faded words are outside the clip.
      </p>
      <div className="flex flex-wrap gap-x-1 gap-y-1.5 text-[14px] leading-6">
        {clip.words.map((w, i) => {
          const inside = i >= start && i <= end;
          const del = deleted.has(i);
          return (
            <span key={i} className="relative" onMouseEnter={() => setHover(i)} onMouseLeave={() => setHover((h) => (h === i ? null : h))}>
              <button
                onClick={() => toggle(i)}
                className={`rounded px-0.5 transition-colors cursor-pointer ${
                  !inside ? "text-dim/60" : del ? "text-coral-soft line-through decoration-2" : "text-paper hover:bg-white/10"
                } ${i === start ? "border-l-2 border-amber pl-1" : ""} ${i === end ? "border-r-2 border-amber pr-1" : ""}`}
              >
                {w.w}
              </button>
              {hover === i && (
                <span className="absolute -top-7 left-0 z-20 flex gap-1">
                  <button onClick={() => setStart(Math.min(i, end))} className="rounded-md bg-amber px-1.5 py-0.5 text-[10.5px] font-bold text-black cursor-pointer">Start</button>
                  <button onClick={() => setEnd(Math.max(i, start))} className="rounded-md bg-amber px-1.5 py-0.5 text-[10.5px] font-bold text-black cursor-pointer">End</button>
                </span>
              )}
            </span>
          );
        })}
      </div>
      <div className="sticky bottom-0 -mx-5 -mb-5 flex items-center justify-between gap-2 border-t hairline bg-ink-900/95 px-5 py-3 backdrop-blur">
        <span className="font-mono text-[12px] text-mute">≈ {fmtDur(dur)}</span>
        <div className="flex gap-2">
          <Button size="sm" tone="ghost" onClick={() => { setStart(clip.auto_range[0]); setEnd(clip.auto_range[1]); setDeleted(new Set()); patch({ trim: null, deleted: [] }); }} disabled={busy}>
            Reset
          </Button>
          <Button size="sm" tone="primary" disabled={!dirty || busy} onClick={() => patch({ trim: [start, end], deleted: [...deleted].sort((a, b) => a - b) })}>
            {busy ? <Loader2 size={13} className="animate-spin" /> : "Apply"}
          </Button>
        </div>
      </div>
    </div>
  );
}
