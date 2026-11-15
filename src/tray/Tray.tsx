import { AnimatePresence, animate, motion, useMotionValue, useTransform, type PanInfo } from "motion/react";
import { Check, ClipboardPaste, Link2, Loader2, Maximize2, Upload, X } from "lucide-react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { useEffect, useMemo, useRef, useState } from "react";
import { Mark } from "../components/Logo";
import { Ring, ScoreDot, Toasts } from "../components/ui";
import { api, errText, fileUrl, fmtDur, isLink, linkHost, on } from "../lib/api";
import { useStore, wireEvents } from "../lib/store";
import type { Clip } from "../lib/types";
import { useHitRects } from "./useHitRects";

const VIDEO_EXT = ["mp4", "mov", "mkv", "webm", "m4v", "avi", "flv", "ts"];

type Item = { pid: string; clip: Clip };

export function Tray() {
  const projects = useStore((s) => s.projects);
  const progress = useStore((s) => s.progress);
  const toast = useStore((s) => s.toast);
  const [near, setNear] = useState(false);
  const [over, setOver] = useState(false);
  const [bubble, setBubble] = useState(false);
  const [deckOpen, setDeckOpen] = useState(false);
  const [hoverPill, setHoverPill] = useState(false);
  const closeTimer = useRef<number | null>(null);
  useHitRects();

  useEffect(() => {
    (async () => {
      await wireEvents();
      await useStore.getState().loadInfo();
      await useStore.getState().loadProjects();
    })();
    const subs = [
      on.dragNear(setNear),
      on.openLink(() => setBubble(true)),
      // Peek the deck when a fresh clip lands.
      on.engine((e) => {
        if (e.kind === "clip" && e.clip.status === "ready") {
          setDeckOpen(true);
          scheduleClose(6000);
        }
      }),
      getCurrentWebview().onDragDropEvent((e) => {
        const t = e.payload.type;
        if (t === "enter" || t === "over") setOver(true);
        else if (t === "leave") setOver(false);
        else if (t === "drop") {
          setOver(false);
          const f = e.payload.paths.find((p) => VIDEO_EXT.includes(p.split(".").pop()?.toLowerCase() ?? ""));
          if (f) start(f);
          else toast("That isn't a video file.", "error");
        }
      }),
    ];
    return () => {
      subs.forEach((s) => s.then((f) => f()));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function scheduleClose(ms = 900) {
    if (closeTimer.current) clearTimeout(closeTimer.current);
    closeTimer.current = window.setTimeout(() => setDeckOpen(false), ms);
  }
  function keepOpen() {
    if (closeTimer.current) clearTimeout(closeTimer.current);
    setDeckOpen(true);
  }

  async function start(input: string) {
    try {
      await api.startJob(input);
      setBubble(false);
      toast(isLink(input) ? `Clipping your ${linkHost(input)} video` : "Clipping your video", "good");
    } catch (e) {
      toast(errText(e), "error");
    }
  }

  // Clips waiting for review (newest video first), and kept ones.
  const { inbox, kept } = useMemo(() => {
    const ps = Object.values(projects).sort((a, b) => b.created - a.created);
    const inbox: Item[] = [];
    const kept: Item[] = [];
    for (const p of ps) {
      for (const c of p.clips.slice().sort((a, b) => b.pick.score - a.pick.score)) {
        if (c.status === "ready") inbox.push({ pid: p.id, clip: c });
        if (c.status === "kept" || c.status === "exporting" || c.status === "exported") kept.push({ pid: p.id, clip: c });
      }
    }
    return { inbox, kept: kept.slice(0, 8) };
  }, [projects]);

  const active = Object.entries(progress)[0];
  const working = !!active;
  const dropping = near || over;

  return (
    <div className="relative h-full w-full overflow-hidden" onMouseLeave={() => deckOpen && scheduleClose()}>
      <AnimatePresence>
        {deckOpen && inbox.length > 0 && !bubble && !dropping && (
          <Deck items={inbox} onEnter={keepOpen} onLeave={() => scheduleClose()} />
        )}
      </AnimatePresence>

      <AnimatePresence>{bubble && <LinkBubble onClose={() => setBubble(false)} onSubmit={start} />}</AnimatePresence>

      <KeptStack items={kept} />

      {/* The pill */}
      <div className="absolute bottom-[18px] right-[18px] flex items-end justify-end">
        <motion.div
          data-hit
          layout
          onMouseEnter={() => {
            setHoverPill(true);
            if (inbox.length) keepOpen();
          }}
          onMouseLeave={() => {
            setHoverPill(false);
            if (deckOpen) scheduleClose();
          }}
          onClick={() => !dropping && setBubble((b) => !b)}
          className="relative flex items-center overflow-hidden cursor-pointer"
          style={{ borderRadius: dropping ? 34 : 999 }}
          animate={{ width: dropping ? 236 : hoverPill && !bubble ? (working ? 300 : 268) : 64, height: dropping ? 236 : 64 }}
          transition={{ type: "spring", stiffness: 420, damping: 32 }}
        >
          <div
            className={`absolute inset-0 ${dropping ? "bg-ink-900/95 border-2 border-dashed border-coral" : "bg-gradient-to-b from-[#ff6d62] to-coral"} transition-colors`}
            style={{ borderRadius: "inherit", boxShadow: dropping ? "0 0 60px rgba(255,90,78,0.45)" : "0 10px 40px -6px rgba(255,90,78,0.7), inset 0 1px 0 rgba(255,255,255,0.35)" }}
          />
          {dropping ? (
            <motion.div initial={{ opacity: 0, scale: 0.8 }} animate={{ opacity: 1, scale: 1 }} className="relative flex h-full w-full flex-col items-center justify-center gap-3 text-center">
              <motion.div animate={{ y: [0, -6, 0] }} transition={{ duration: 1.2, repeat: Infinity }}>
                <Upload size={34} className="text-coral" />
              </motion.div>
              <div className="font-display text-[19px] font-bold">Drop to clip</div>
              <div className="px-6 text-[12px] text-mute">Cutdeck will find the best moments</div>
            </motion.div>
          ) : (
            <>
              <AnimatePresence>
                {hoverPill && !bubble && (
                  <motion.div initial={{ opacity: 0, x: 10 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0 }} className="relative min-w-0 flex-1 whitespace-nowrap pl-5 pr-2 text-white">
                    {working ? (
                      <>
                        <div className="truncate text-[13px] font-semibold">{active[1].label}</div>
                        <div className="truncate text-[11.5px] text-white/75">{active[1].detail}</div>
                      </>
                    ) : (
                      <>
                        <div className="text-[13px] font-semibold">{inbox.length ? `${inbox.length} clips to review` : "Click to paste a link"}</div>
                        <div className="text-[11.5px] text-white/75">or drop a video here</div>
                      </>
                    )}
                  </motion.div>
                )}
              </AnimatePresence>
              <div className="relative ml-auto grid h-16 w-16 shrink-0 place-items-center">
                {working && <Ring value={active[1].progress} size={64} stroke={4} className="absolute inset-0 drop-shadow" />}
                {working ? (
                  <span className="font-mono text-[12px] font-bold text-white">{Math.round(active[1].progress * 100)}</span>
                ) : (
                  <motion.div animate={{ rotate: hoverPill ? -8 : 0 }}>
                    <Mark size={34} />
                  </motion.div>
                )}
              </div>
            </>
          )}
          {!dropping && inbox.length > 0 && !hoverPill && (
            <motion.span initial={{ scale: 0 }} animate={{ scale: 1 }} className="absolute right-0 top-0 grid h-5 min-w-5 place-items-center rounded-full bg-paper px-1 text-[11px] font-bold text-ink-950 shadow">
              {inbox.length}
            </motion.span>
          )}
        </motion.div>
      </div>
      <Toasts compact />
    </div>
  );
}

function LinkBubble({ onClose, onSubmit }: { onClose: () => void; onSubmit: (s: string) => void }) {
  const [v, setV] = useState("");
  const [clip, setClip] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    api.focusTray().then(() => setTimeout(() => ref.current?.focus(), 30));
    readText()
      .then((t) => {
        if (t && isLink(t)) setClip(t.trim());
      })
      .catch(() => {});
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);
  const submit = async (s: string) => {
    setBusy(true);
    await onSubmit(s);
    setBusy(false);
  };
  return (
    <motion.div
      data-hit
      initial={{ opacity: 0, y: 20, scale: 0.92, originX: 1, originY: 1 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={{ opacity: 0, y: 12, scale: 0.94, transition: { duration: 0.14 } }}
      transition={{ type: "spring", stiffness: 420, damping: 30 }}
      className="glass-dark absolute bottom-[96px] right-[18px] w-[400px] rounded-[26px] p-4 shadow-[0_30px_80px_-20px_rgba(0,0,0,0.9)]"
    >
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-2 font-display text-[16px] font-semibold"><Mark size={22} /> Clip a video</div>
        <div className="flex gap-1">
          <button onClick={() => api.openStudio()} className="grid h-7 w-7 place-items-center rounded-lg text-mute hover:bg-white/10 hover:text-paper cursor-pointer" title="Open Studio"><Maximize2 size={13} /></button>
          <button onClick={onClose} className="grid h-7 w-7 place-items-center rounded-lg text-mute hover:bg-white/10 hover:text-paper cursor-pointer"><X size={14} /></button>
        </div>
      </div>
      <AnimatePresence>
        {clip && !v && (
          <motion.button
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            onClick={() => submit(clip)}
            className="mb-2.5 flex w-full items-center gap-3 rounded-2xl border border-coral/30 bg-coral/10 px-3.5 py-2.5 text-left hover:bg-coral/15 cursor-pointer"
          >
            <ClipboardPaste size={16} className="shrink-0 text-coral" />
            <span className="min-w-0 flex-1">
              <span className="block text-[13px] font-semibold">Clip the {linkHost(clip)} link you copied</span>
              <span className="block truncate font-mono text-[11px] text-mute">{clip}</span>
            </span>
          </motion.button>
        )}
      </AnimatePresence>
      <div className="flex gap-2">
        <div className="relative flex-1">
          <Link2 size={15} className={`absolute left-3.5 top-1/2 -translate-y-1/2 ${isLink(v) ? "text-coral" : "text-dim"}`} />
          <input
            ref={ref}
            value={v}
            onChange={(e) => setV(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && isLink(v) && submit(v)}
            placeholder="Paste a YouTube or Kick link"
            spellCheck={false}
            className="h-11 w-full rounded-xl border border-white/[0.09] bg-black/40 pl-10 pr-3 text-[13.5px] outline-none placeholder:text-dim focus:border-coral/60"
          />
        </div>
        <button
          disabled={!isLink(v) || busy}
          onClick={() => submit(v)}
          className="grid h-11 w-11 place-items-center rounded-xl bg-gradient-to-b from-[#ff6d62] to-coral text-white disabled:opacity-35 cursor-pointer"
        >
          {busy ? <Loader2 size={16} className="animate-spin" /> : <Check size={18} />}
        </button>
      </div>
      <div className="mt-2.5 text-[11.5px] text-dim">Or drag a video file onto the coral button.</div>
    </motion.div>
  );
}

function Deck({ items, onEnter, onLeave }: { items: Item[]; onEnter: () => void; onLeave: () => void }) {
  const top = items[0];
  const rest = items.slice(1, 4);
  const toast = useStore((s) => s.toast);
  async function decide(it: Item, keep: boolean) {
    const st = useStore.getState();
    st.upsertClip(it.pid, { ...it.clip, status: keep ? "kept" : "discarded" });
    try {
      if (keep) await api.keepClip(it.pid, it.clip.id);
      else await api.discardClip(it.pid, it.clip.id);
    } catch (e) {
      toast(errText(e), "error");
      st.upsertClip(it.pid, it.clip);
    }
  }
  return (
    <motion.div
      data-hit
      onMouseEnter={onEnter}
      onMouseLeave={onLeave}
      initial={{ opacity: 0, x: 40, y: 30, scale: 0.85 }}
      animate={{ opacity: 1, x: 0, y: 0, scale: 1 }}
      exit={{ opacity: 0, x: 60, y: 40, scale: 0.85, transition: { duration: 0.2 } }}
      transition={{ type: "spring", stiffness: 300, damping: 26 }}
      className="absolute bottom-[100px] right-[100px] flex flex-col items-center"
    >
      <div className="relative h-[384px] w-[216px]">
        {rest
          .slice()
          .reverse()
          .map((it, ri) => {
            const i = rest.length - ri;
            return (
              <motion.div
                key={`${it.pid}/${it.clip.id}`}
                className="absolute inset-0 overflow-hidden rounded-[22px] border border-white/10 bg-ink-800 shadow-2xl"
                animate={{ rotate: i * 4.5, x: i * 12, y: i * 4, scale: 1 - i * 0.04 }}
                transition={{ type: "spring", stiffness: 260, damping: 24 }}
              >
                {it.clip.thumb && <img src={fileUrl(it.clip.thumb, it.clip.rev)} className="h-full w-full object-cover opacity-60" />}
              </motion.div>
            );
          })}
        <AnimatePresence>
          <TopCard key={`${top.pid}/${top.clip.id}`} it={top} onDecide={(k) => decide(top, k)} />
        </AnimatePresence>
      </div>
      <div className="mt-4 flex items-center gap-3">
        <button onClick={() => decide(top, false)} className="glass-dark grid h-12 w-12 place-items-center rounded-full text-mute hover:text-paper hover:scale-105 transition cursor-pointer" title="Skip">
          <X size={20} />
        </button>
        <button onClick={() => api.openStudio(`project:${top.pid}`)} className="glass-dark rounded-full px-4 py-2 text-[12px] text-mute hover:text-paper cursor-pointer">
          {items.length} to review · Edit
        </button>
        <button onClick={() => decide(top, true)} className="grid h-12 w-12 place-items-center rounded-full bg-gradient-to-b from-[#ff6d62] to-coral text-white shadow-[0_8px_30px_-6px_rgba(255,90,78,0.8)] hover:scale-105 transition cursor-pointer" title="Keep">
          <Check size={22} />
        </button>
      </div>
    </motion.div>
  );
}

function TopCard({ it, onDecide }: { it: Item; onDecide: (keep: boolean) => void }) {
  const x = useMotionValue(0);
  const rotate = useTransform(x, [-200, 200], [-16, 16]);
  const keepOp = useTransform(x, [20, 110], [0, 1]);
  const skipOp = useTransform(x, [-110, -20], [1, 0]);
  const vid = useRef<HTMLVideoElement>(null);
  const [gone, setGone] = useState<null | "keep" | "skip">(null);

  function end(_: unknown, info: PanInfo) {
    const dx = info.offset.x;
    const vx = info.velocity.x;
    if (dx > 110 || vx > 600) fly(true);
    else if (dx < -110 || vx < -600) fly(false);
    else animate(x, 0, { type: "spring", stiffness: 500, damping: 30 });
  }
  function fly(keep: boolean) {
    setGone(keep ? "keep" : "skip");
    animate(x, keep ? 520 : -520, { duration: 0.35, ease: [0.3, 0, 0.6, 1] }).then(() => onDecide(keep));
  }

  return (
    <motion.div
      drag="x"
      dragElastic={0.9}
      dragConstraints={{ left: 0, right: 0 }}
      onDragEnd={end}
      style={{ x, rotate }}
      initial={{ scale: 0.9, opacity: 0, y: 20 }}
      animate={{ scale: 1, opacity: 1, y: gone === "keep" ? -80 : gone === "skip" ? 60 : 0 }}
      exit={{ opacity: 0 }}
      whileTap={{ cursor: "grabbing" }}
      transition={{ type: "spring", stiffness: 320, damping: 26 }}
      onMouseEnter={() => vid.current?.play().catch(() => {})}
      onMouseLeave={() => vid.current?.pause()}
      className="absolute inset-0 z-10 cursor-grab overflow-hidden rounded-[22px] border border-white/15 bg-black shadow-[0_30px_70px_-15px_rgba(0,0,0,0.95)]"
    >
      <video ref={vid} src={fileUrl(it.clip.preview, it.clip.rev)} poster={fileUrl(it.clip.thumb, it.clip.rev)} muted loop playsInline className="pointer-events-none h-full w-full object-cover" />
      <div className="pointer-events-none absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/90 via-black/50 to-transparent p-3.5 pt-14">
        <div className="line-clamp-2 text-[13.5px] font-semibold leading-snug">{it.clip.pick.title}</div>
        <div className="mt-1.5 flex items-center gap-2 text-[11px] text-white/70">
          <ScoreDot score={it.clip.pick.score} /> <span className="font-mono">{fmtDur(it.clip.duration)}</span>
        </div>
      </div>
      <motion.div style={{ opacity: keepOp }} className="pointer-events-none absolute left-4 top-5 -rotate-12 rounded-lg border-[3px] border-mint px-2.5 py-0.5 font-display text-[22px] font-extrabold tracking-wide text-mint">
        KEEP
      </motion.div>
      <motion.div style={{ opacity: skipOp }} className="pointer-events-none absolute right-4 top-5 rotate-12 rounded-lg border-[3px] border-white/70 px-2.5 py-0.5 font-display text-[22px] font-extrabold tracking-wide text-white/80">
        SKIP
      </motion.div>
    </motion.div>
  );
}

function KeptStack({ items }: { items: Item[] }) {
  const [open, setOpen] = useState(false);
  const exp = useStore((s) => s.exportProgress);
  if (items.length === 0) return null;
  return (
    <motion.div
      data-hit
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
      className="absolute bottom-[18px] right-[96px] flex h-16 items-center"
      animate={{ width: open ? Math.min(items.length, 6) * 50 + 20 : 58 }}
      transition={{ type: "spring", stiffness: 400, damping: 32 }}
    >
      <div className="relative h-16 w-full">
        {items.slice(0, 6).map((it, i) => {
          const done = it.clip.status === "exported" && it.clip.export_path;
          const p = exp[`${it.pid}/${it.clip.id}`];
          return (
            <motion.div
              key={`${it.pid}/${it.clip.id}`}
              className={`absolute bottom-0 h-16 w-9 overflow-hidden rounded-lg border border-white/20 bg-ink-800 shadow-xl ${done ? "cursor-grab" : ""}`}
              style={{ zIndex: 10 - i }}
              animate={{ right: open ? i * 50 + 8 : i * 3, rotate: open ? 0 : -i * 5, y: open ? 0 : i * -1 }}
              transition={{ type: "spring", stiffness: 380, damping: 28 }}
              title={done ? "Drag into any app" : "Exporting…"}
              onPointerDown={(e) => {
                if (!done) return;
                e.preventDefault();
                startDrag({ item: [it.clip.export_path!], icon: it.clip.thumb ?? "" }).catch(() => {});
              }}
              onDoubleClick={() => done && api.reveal(it.clip.export_path!)}
            >
              {it.clip.thumb && <img src={fileUrl(it.clip.thumb, it.clip.rev)} className={`h-full w-full object-cover ${done ? "" : "opacity-50"}`} draggable={false} />}
              {!done && (
                <div className="absolute inset-0 grid place-items-center">
                  <Ring value={p ?? 0.02} size={24} stroke={3} />
                </div>
              )}
              {done && open && <Check size={11} className="absolute right-0.5 top-0.5 rounded-full bg-mint p-px text-black" />}
            </motion.div>
          );
        })}
      </div>
    </motion.div>
  );
}
