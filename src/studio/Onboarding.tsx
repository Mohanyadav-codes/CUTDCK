import { AnimatePresence, motion } from "motion/react";
import { ArrowRight, Check, Loader2, MousePointer2, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { AiConnect } from "../components/AiConnect";
import { DeckHero } from "../components/DeckHero";
import { Mark } from "../components/Logo";
import { Button } from "../components/ui";
import { api, errText, on } from "../lib/api";
import { useStore } from "../lib/store";
import type { DepProgress } from "../lib/types";

const STEPS = ["Welcome", "Your AI", "Set up", "Ready"];

function Steps({ step }: { step: number }) {
  return (
    <div className="flex items-center gap-2">
      {STEPS.map((s, i) => (
        <div key={s} className="flex items-center gap-2">
          <div className={`flex items-center gap-2 text-[12px] font-medium transition-colors ${i === step ? "text-paper" : i < step ? "text-mute" : "text-dim"}`}>
            <span className={`grid h-5 w-5 place-items-center rounded-full text-[10.5px] font-bold transition-all ${i < step ? "bg-mint/20 text-mint" : i === step ? "bg-coral text-white" : "bg-white/[0.06] text-dim"}`}>
              {i < step ? <Check size={11} strokeWidth={3} /> : i + 1}
            </span>
            {s}
          </div>
          {i < STEPS.length - 1 && <div className="h-px w-6 bg-white/10" />}
        </div>
      ))}
    </div>
  );
}

const slide = {
  initial: { opacity: 0, x: 40, filter: "blur(6px)" },
  animate: { opacity: 1, x: 0, filter: "blur(0px)" },
  exit: { opacity: 0, x: -40, filter: "blur(6px)" },
  transition: { type: "spring" as const, stiffness: 220, damping: 26 },
};

function Welcome({ next }: { next: () => void }) {
  const styles = useStore((s) => s.info?.styles ?? []);
  return (
    <motion.div {...slide} className="flex flex-col items-center text-center">
      <DeckHero styles={styles} />
      <h1 className="mt-2 font-display text-[52px] font-bold leading-[0.95] tracking-[-0.045em]">
        Long videos in.
        <br />
        <span className="text-gradient">Shorts out.</span>
      </h1>
      <p className="mt-5 max-w-[520px] text-[16px] leading-relaxed text-mute">
        Paste a YouTube link or drop a video. Cutdeck finds the moments worth posting, frames them for vertical and adds captions, right on your computer.
      </p>
      <Button tone="primary" size="lg" className="mt-7 px-8" onClick={next}>
        Get started <ArrowRight size={17} />
      </Button>
    </motion.div>
  );
}

function ConnectAi({ next }: { next: () => void }) {
  return (
    <motion.div {...slide} className="w-[760px]">
      <h2 className="font-display text-[40px] font-bold tracking-[-0.035em]">Connect your AI</h2>
      <p className="mb-7 mt-2 max-w-[600px] text-[15px] leading-relaxed text-mute">
        Cutdeck uses an AI model to listen to the video and pick the moments. A free Groq key covers everything and takes a minute to get.
      </p>
      <div className="glass rounded-3xl p-6">
        <AiConnect onSaved={next} />
      </div>
    </motion.div>
  );
}

const ITEMS = [
  { item: "ffmpeg", label: "Video engine", note: "FFmpeg" },
  { item: "ytdlp", label: "Link reader", note: "yt-dlp" },
  { item: "deno", label: "YouTube helper", note: "Deno" },
];

function Setup({ next }: { next: () => void }) {
  const info = useStore((s) => s.info);
  const loadInfo = useStore((s) => s.loadInfo);
  const [prog, setProg] = useState<Record<string, DepProgress>>({});
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const done = (k: string) => {
    const d = info?.deps;
    if (prog[k]?.finished) return true;
    if (!d) return false;
    return k === "ffmpeg" ? d.ffmpeg : k === "ytdlp" ? d.ytdlp : d.deno;
  };
  const allDone = ITEMS.every((i) => done(i.item));

  async function run() {
    setError(null);
    setRunning(true);
    try {
      await api.installDeps();
      await loadInfo();
    } catch (e) {
      setError(errText(e));
    } finally {
      setRunning(false);
    }
  }

  useEffect(() => {
    const un = on.deps((p) => setProg((s) => ({ ...s, [p.item]: p })));
    if (!allDone) run();
    return () => {
      un.then((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <motion.div {...slide} className="w-[620px]">
      <h2 className="font-display text-[40px] font-bold tracking-[-0.035em]">{allDone ? "All set up" : "Setting up your deck"}</h2>
      <p className="mb-7 mt-2 text-[15px] leading-relaxed text-mute">
        Cutdeck downloads the tools it needs once, about 150 MB. Your videos are processed on this computer.
      </p>
      <div className="flex flex-col gap-3">
        {ITEMS.map((it, i) => {
          const p = prog[it.item];
          const ok = done(it.item);
          const frac = ok ? 1 : p && p.total > 0 ? p.done / p.total : 0;
          return (
            <motion.div
              key={it.item}
              initial={{ opacity: 0, y: 20, rotate: -2 }}
              animate={{ opacity: 1, y: 0, rotate: 0 }}
              transition={{ delay: 0.1 + i * 0.1, type: "spring", stiffness: 200, damping: 20 }}
              className="glass relative overflow-hidden rounded-2xl px-5 py-4"
            >
              <motion.div className="absolute inset-y-0 left-0 bg-gradient-to-r from-coral/20 to-amber/10" animate={{ width: `${frac * 100}%` }} transition={{ type: "spring", stiffness: 80, damping: 20 }} />
              <div className="relative flex items-center gap-4">
                <span className={`grid h-9 w-9 place-items-center rounded-xl ${ok ? "bg-mint/15 text-mint" : "bg-white/[0.06] text-mute"}`}>
                  {ok ? <Check size={18} strokeWidth={2.5} /> : p ? <Loader2 size={17} className="animate-spin" /> : <span className="h-1.5 w-1.5 rounded-full bg-dim" />}
                </span>
                <div className="flex-1">
                  <div className="text-[15px] font-medium">{it.label}</div>
                  <div className="text-[12px] text-dim">{it.note}</div>
                </div>
                <div className="font-mono text-[12px] text-mute">
                  {ok ? "Ready" : p && p.total > 0 ? `${(p.done / 1048576).toFixed(0)} / ${(p.total / 1048576).toFixed(0)} MB` : running ? "Waiting" : ""}
                </div>
              </div>
            </motion.div>
          );
        })}
      </div>
      {error && (
        <div className="mt-4 flex items-center justify-between gap-4 rounded-xl border border-coral/25 bg-coral/[0.07] px-4 py-3 text-[13px] text-coral-soft">
          <span>{error}</span>
          <Button size="sm" tone="soft" onClick={run}><RefreshCw size={13} /> Retry</Button>
        </div>
      )}
      <div className="mt-8 flex justify-end">
        <Button tone="primary" size="lg" disabled={!allDone} onClick={next}>
          Continue <ArrowRight size={17} />
        </Button>
      </div>
    </motion.div>
  );
}

function Ready({ finish }: { finish: () => void }) {
  return (
    <motion.div {...slide} className="flex w-[720px] flex-col items-center text-center">
      <div className="relative mb-8 h-[230px] w-[420px] overflow-hidden rounded-3xl border border-white/10 bg-gradient-to-br from-[#16161f] to-[#0b0b10]">
        <div className="absolute inset-x-0 top-0 h-7 border-b border-white/5 bg-white/[0.03]" />
        <div className="absolute left-6 top-12 h-3 w-40 rounded bg-white/[0.06]" />
        <div className="absolute left-6 top-20 h-3 w-56 rounded bg-white/[0.04]" />
        <div className="absolute left-6 top-28 h-3 w-32 rounded bg-white/[0.04]" />
        {[0, 1, 2].map((i) => (
          <motion.div
            key={i}
            className="absolute bottom-6 right-24 h-[96px] w-[54px] rounded-xl border border-white/10"
            style={{ background: ["#3b2a8f", "#a0442a", "#0f6b6b"][i] }}
            initial={{ opacity: 0, x: 40, rotate: 0 }}
            animate={{ opacity: 1, x: -i * 34, rotate: -6 - i * 6 }}
            transition={{ delay: 0.9 + i * 0.12, type: "spring", stiffness: 160, damping: 16 }}
          />
        ))}
        <motion.div
          className="absolute bottom-6 right-6 grid h-14 w-14 place-items-center rounded-full bg-gradient-to-b from-[#ff6d62] to-coral shadow-[0_0_40px_rgba(255,90,78,0.6)]"
          initial={{ scale: 0 }}
          animate={{ scale: 1 }}
          transition={{ delay: 0.3, type: "spring", stiffness: 300, damping: 15 }}
        >
          <Mark size={30} />
        </motion.div>
        <motion.div className="absolute text-paper" initial={{ left: 140, top: 60 }} animate={{ left: 370, top: 170 }} transition={{ delay: 0.5, duration: 0.6, ease: "easeInOut" }}>
          <MousePointer2 size={20} fill="white" />
        </motion.div>
      </div>
      <h2 className="font-display text-[44px] font-bold leading-tight tracking-[-0.035em]">Your deck lives in the corner</h2>
      <p className="mt-3 max-w-[540px] text-[15px] leading-relaxed text-mute">
        Drop a video on the coral button, or click it and paste a link. When clips are ready they fan out as cards: flick right to keep, left to skip. Kept clips can be dragged straight into any app.
      </p>
      <Button tone="primary" size="lg" className="mt-8 px-8" onClick={finish}>
        Open my deck <ArrowRight size={17} />
      </Button>
    </motion.div>
  );
}

export function Onboarding({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState(0);
  const next = () => setStep((s) => Math.min(3, s + 1));
  async function finish() {
    await api.finishOnboarding();
    onDone();
  }
  return (
    <div className="relative z-10 flex min-h-0 flex-1 flex-col items-center">
      <div className="mt-2"><Steps step={step} /></div>
      <div className="scroll-thin flex min-h-0 flex-1 items-center justify-center overflow-y-auto overflow-x-hidden py-6">
        <AnimatePresence mode="wait">
          {step === 0 && <Welcome key="w" next={next} />}
          {step === 1 && <ConnectAi key="a" next={next} />}
          {step === 2 && <Setup key="s" next={next} />}
          {step === 3 && <Ready key="r" finish={finish} />}
        </AnimatePresence>
      </div>
    </div>
  );
}
