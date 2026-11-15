import { AnimatePresence, motion } from "motion/react";
import { Film, Home, Loader2, Settings as Cog } from "lucide-react";
import { useEffect, useState } from "react";
import { Wordmark } from "../components/Logo";
import { Toasts } from "../components/ui";
import { fileUrl, on } from "../lib/api";
import { useStore, wireEvents } from "../lib/store";
import { Backdrop } from "./Backdrop";
import { Library } from "./Library";
import { Onboarding } from "./Onboarding";
import { ProjectView } from "./ProjectView";
import { SettingsView } from "./SettingsView";
import { TitleBar } from "./TitleBar";

export type View = { kind: "library" } | { kind: "project"; id: string } | { kind: "settings" };

export function Studio() {
  const info = useStore((s) => s.info);
  const loadInfo = useStore((s) => s.loadInfo);
  const loadProjects = useStore((s) => s.loadProjects);
  const [view, setView] = useState<View>({ kind: "library" });
  const [ready, setReady] = useState(false);

  useEffect(() => {
    (async () => {
      await wireEvents();
      await loadInfo();
      await loadProjects();
      setReady(true);
    })();
    const un = on.navigate((v) => {
      if (v === "settings") setView({ kind: "settings" });
      else if (v.startsWith("project:")) setView({ kind: "project", id: v.slice(8) });
      else setView({ kind: "library" });
    });
    return () => {
      un.then((f) => f());
    };
  }, [loadInfo, loadProjects]);

  const onboarded = !!info?.settings.onboarded && !!info?.settings.ai;

  return (
    <div className="relative flex h-full flex-col overflow-hidden bg-ink-950">
      <Backdrop intensity={onboarded ? 0.7 : 1} />
      <TitleBar>{onboarded && <Wordmark size={17} />}</TitleBar>
      {!ready ? (
        <div className="grid flex-1 place-items-center text-mute"><Loader2 className="animate-spin" /></div>
      ) : !onboarded ? (
        <Onboarding onDone={async () => { await loadInfo(); setView({ kind: "library" }); }} />
      ) : (
        <div className="relative z-10 flex min-h-0 flex-1">
          <Sidebar view={view} setView={setView} />
          <main className="relative min-w-0 flex-1 overflow-hidden">
            <AnimatePresence mode="wait">
              <motion.div
                key={view.kind === "project" ? view.id : view.kind}
                className="absolute inset-0"
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -6 }}
                transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
              >
                {view.kind === "library" && <Library open={(id) => setView({ kind: "project", id })} />}
                {view.kind === "project" && <ProjectView id={view.id} back={() => setView({ kind: "library" })} />}
                {view.kind === "settings" && <SettingsView />}
              </motion.div>
            </AnimatePresence>
          </main>
        </div>
      )}
      <Toasts />
    </div>
  );
}

function Sidebar({ view, setView }: { view: View; setView: (v: View) => void }) {
  const projects = useStore((s) => s.projects);
  const progress = useStore((s) => s.progress);
  const list = Object.values(projects).sort((a, b) => b.created - a.created).slice(0, 12);
  const item = (active: boolean) =>
    `group flex w-full items-center gap-3 rounded-xl px-3 h-10 text-[13.5px] font-medium transition-colors cursor-pointer ${active ? "bg-white/[0.08] text-paper" : "text-mute hover:bg-white/[0.04] hover:text-paper"}`;
  return (
    <aside className="flex w-[244px] shrink-0 flex-col gap-1 border-r hairline px-3 pb-4 pt-2">
      <button className={item(view.kind === "library")} onClick={() => setView({ kind: "library" })}>
        <Home size={16} /> Home
      </button>
      <div className="mb-1 mt-5 px-3 text-[11px] font-semibold uppercase tracking-[0.12em] text-dim">Videos</div>
      <div className="scroll-thin -mx-1 flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto px-1">
        {list.length === 0 && <div className="px-3 py-2 text-[12.5px] text-dim">Nothing yet</div>}
        {list.map((p) => {
          const prog = progress[p.id];
          const thumb = p.meta?.thumbnail ?? fileUrl(p.clips.find((c) => c.thumb)?.thumb, 0);
          const active = view.kind === "project" && view.id === p.id;
          return (
            <button key={p.id} className={item(active) + " !h-12"} onClick={() => setView({ kind: "project", id: p.id })}>
              <span className="relative h-8 w-12 shrink-0 overflow-hidden rounded-md bg-white/[0.06]">
                {thumb ? <img src={thumb} className="h-full w-full object-cover" /> : <Film size={14} className="absolute inset-0 m-auto text-dim" />}
                {prog && <span className="absolute inset-x-0 bottom-0 h-0.5 bg-white/20"><span className="block h-full bg-amber" style={{ width: `${prog.progress * 100}%` }} /></span>}
              </span>
              <span className="min-w-0 flex-1 text-left">
                <span className="block truncate text-[13px]">{p.meta?.title ?? "Opening…"}</span>
                <span className="block truncate text-[11.5px] font-normal text-dim">
                  {prog ? prog.label : p.error ? "Stopped" : `${p.clips.filter((c) => c.status !== "discarded").length} clips`}
                </span>
              </span>
            </button>
          );
        })}
      </div>
      <button className={item(view.kind === "settings")} onClick={() => setView({ kind: "settings" })}>
        <Cog size={16} /> Settings
      </button>
    </aside>
  );
}
