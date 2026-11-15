import { AnimatePresence, motion } from "motion/react";
import { ArrowUpRight, Check, Cpu, Eye, EyeOff, HardDrive, KeyRound, Loader2, Sparkles } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useMemo, useState } from "react";
import { api, errText, on } from "../lib/api";
import { useStore } from "../lib/store";
import type { AiConfig, Preset } from "../lib/types";
import { Button, Segmented } from "./ui";

const ORDER = ["groq", "gemini", "xai", "openai", "openrouter"];

function KeyInput({ value, onChange, placeholder, autoFocus }: { value: string; onChange: (v: string) => void; placeholder: string; autoFocus?: boolean }) {
  const [show, setShow] = useState(false);
  return (
    <div className="relative flex-1">
      <KeyRound size={15} className="absolute left-3.5 top-1/2 -translate-y-1/2 text-dim" />
      <input
        autoFocus={autoFocus}
        type={show ? "text" : "password"}
        value={value}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="h-11 w-full rounded-xl bg-black/35 border border-white/[0.09] pl-10 pr-10 font-mono text-[13px] text-paper placeholder:text-dim placeholder:font-sans outline-none focus:border-coral/60 focus:shadow-[0_0_0_4px_rgba(255,90,78,0.12)] transition"
      />
      <button className="absolute right-3 top-1/2 -translate-y-1/2 text-dim hover:text-paper cursor-pointer" onClick={() => setShow(!show)} tabIndex={-1}>
        {show ? <EyeOff size={15} /> : <Eye size={15} />}
      </button>
    </div>
  );
}

/** Choose and verify an AI provider. Calls `onSaved` once the config is stored. */
export function AiConnect({ onSaved, compact = false }: { onSaved?: (cfg: AiConfig) => void; compact?: boolean }) {
  const info = useStore((s) => s.info);
  const loadInfo = useStore((s) => s.loadInfo);
  const presets = info?.presets ?? [];
  const current = info?.settings.ai ?? null;
  const [mode, setMode] = useState<"cloud" | "local">(current?.provider === "local" ? "local" : "cloud");
  const [provider, setProvider] = useState(current && current.provider !== "local" ? current.provider : "groq");
  const [key, setKey] = useState("");
  const [baseUrl, setBaseUrl] = useState(current?.provider === "local" ? current.base_url : "http://localhost:11434/v1");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [model, setModel] = useState<string | null>(current?.model ?? null);
  const [verified, setVerified] = useState<string | null>(null);
  const [sttChoice, setSttChoice] = useState<"groq" | "local">(current?.stt === "local" ? "local" : "groq");
  const [groqKey, setGroqKey] = useState("");
  const [whisperSize, setWhisperSize] = useState(current?.whisper_size ?? "base");
  const [whisperBusy, setWhisperBusy] = useState<number | null>(null);

  const pid = mode === "local" ? "local" : provider;
  const pre: Preset | undefined = presets.find((p) => p.id === pid);
  const hasStt = !!pre?.stt_model;
  const whisperReady = !!info?.deps.whisper && (info?.deps.whisper_models.length ?? 0) > 0;
  const savedKey = current?.provider === pid && info?.has_key;

  useEffect(() => {
    setVerified(null);
    setModels([]);
    setError(null);
  }, [pid]);

  useEffect(() => {
    const un = on.deps((p) => {
      if (p.item.startsWith("whisper")) setWhisperBusy(p.total > 0 ? p.done / p.total : 0);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  const sorted = useMemo(() => ORDER.map((id) => presets.find((p) => p.id === id)).filter(Boolean) as Preset[], [presets]);

  async function verify() {
    setBusy(true);
    setError(null);
    try {
      const r = await api.testAi(pid, mode === "local" ? baseUrl : null, key || null);
      setModels(r.models);
      setModel(r.chosen);
      setVerified(r.chosen);
    } catch (e) {
      setError(errText(e));
      setVerified(null);
    } finally {
      setBusy(false);
    }
  }

  async function installWhisper() {
    setWhisperBusy(0);
    setError(null);
    try {
      await api.installWhisper(whisperSize);
      await loadInfo();
    } catch (e) {
      setError(errText(e));
    } finally {
      setWhisperBusy(null);
    }
  }

  const sttOk = hasStt || (sttChoice === "groq" ? groqKey.trim().length > 10 || (current?.stt_provider === "groq" && info?.has_key) : whisperReady);
  const canSave = !!verified && !!model && sttOk;

  async function save() {
    if (!pre || !model) return;
    const cfg: AiConfig = {
      provider: pid,
      base_url: mode === "local" ? baseUrl : pre.base_url,
      model,
      stt: hasStt ? "cloud" : sttChoice === "groq" ? "cloud" : "local",
      whisper_size: whisperSize,
      stt_provider: hasStt ? null : sttChoice === "groq" ? "groq" : null,
    };
    setBusy(true);
    try {
      await api.saveAi(cfg, key || null, !hasStt && sttChoice === "groq" ? groqKey || null : null);
      await loadInfo();
      onSaved?.(cfg);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <Segmented
        value={mode}
        onChange={setMode}
        options={[
          { value: "cloud", label: <span className="inline-flex items-center gap-1.5"><Sparkles size={13} /> Paste an API key</span> },
          { value: "local", label: <span className="inline-flex items-center gap-1.5"><Cpu size={13} /> Open-source model</span> },
        ]}
      />

      <AnimatePresence mode="wait">
        {mode === "cloud" ? (
          <motion.div key="cloud" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }} transition={{ duration: 0.18 }} className="flex flex-col gap-4">
            <div className={`grid gap-2.5 ${compact ? "grid-cols-5" : "grid-cols-5"}`}>
              {sorted.map((p) => {
                const sel = p.id === provider;
                return (
                  <button
                    key={p.id}
                    onClick={() => setProvider(p.id)}
                    className={`relative flex flex-col items-start gap-1 rounded-2xl border px-3.5 py-3 text-left transition-all cursor-pointer ${
                      sel ? "border-coral/60 bg-coral/[0.08] shadow-[0_0_0_4px_rgba(255,90,78,0.08)]" : "border-white/[0.07] bg-white/[0.025] hover:bg-white/[0.05]"
                    }`}
                  >
                    <span className="font-display text-[15px] font-semibold">{p.name}</span>
                    <span className="flex flex-wrap gap-1">
                      {p.id === "groq" && <span className="rounded-full bg-coral px-1.5 py-px text-[9.5px] font-bold uppercase tracking-wider text-white">Best</span>}
                      {p.free && <span className="rounded-full bg-mint/15 px-1.5 py-px text-[9.5px] font-bold uppercase tracking-wider text-mint">Free</span>}
                    </span>
                    {sel && (
                      <motion.span layoutId="prov-check" className="absolute right-2.5 top-2.5 grid h-4 w-4 place-items-center rounded-full bg-coral">
                        <Check size={10} strokeWidth={3} />
                      </motion.span>
                    )}
                  </button>
                );
              })}
            </div>
            {pre && (
              <div className="flex flex-col gap-2">
                <div className="flex items-center justify-between text-[12.5px] text-mute">
                  <span>{pre.tagline}</span>
                  <button className="inline-flex items-center gap-1 text-coral-soft hover:text-coral cursor-pointer" onClick={() => openUrl(pre.key_url)}>
                    Get a {pre.free ? "free " : ""}key <ArrowUpRight size={13} />
                  </button>
                </div>
                <div className="flex gap-2">
                  <KeyInput value={key} onChange={setKey} placeholder={savedKey ? "Key saved. Paste a new one to replace it." : `Paste your ${pre.name} key`} autoFocus={!compact} />
                  <Button tone={verified ? "soft" : "primary"} onClick={verify} disabled={busy || (!key && !savedKey)} className="w-32">
                    {busy ? <Loader2 size={16} className="animate-spin" /> : verified ? <><Check size={16} className="text-mint" /> Connected</> : "Connect"}
                  </Button>
                </div>
              </div>
            )}
          </motion.div>
        ) : (
          <motion.div key="local" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }} transition={{ duration: 0.18 }} className="flex flex-col gap-3">
            <p className="text-[13px] leading-relaxed text-mute">
              Run any open-source model yourself with{" "}
              <button className="text-coral-soft hover:text-coral cursor-pointer" onClick={() => openUrl("https://ollama.com/download")}>Ollama</button> or{" "}
              <button className="text-coral-soft hover:text-coral cursor-pointer" onClick={() => openUrl("https://lmstudio.ai")}>LM Studio</button>, then point Cutdeck at it.
              A 7-14B model such as <span className="font-mono text-paper/80">qwen3:8b</span> works well. Nothing leaves your computer.
            </p>
            <div className="flex gap-2">
              <input
                value={baseUrl}
                spellCheck={false}
                onChange={(e) => setBaseUrl(e.target.value)}
                className="h-11 flex-1 rounded-xl bg-black/35 border border-white/[0.09] px-3.5 font-mono text-[13px] outline-none focus:border-coral/60"
              />
              <Button tone={verified ? "soft" : "primary"} onClick={verify} disabled={busy} className="w-32">
                {busy ? <Loader2 size={16} className="animate-spin" /> : verified ? <><Check size={16} className="text-mint" /> Found</> : "Detect"}
              </Button>
            </div>
            <div className="flex gap-2 text-[12px]">
              {["http://localhost:11434/v1", "http://localhost:1234/v1"].map((u) => (
                <button key={u} onClick={() => setBaseUrl(u)} className="rounded-lg bg-white/[0.04] px-2.5 py-1 font-mono text-mute hover:text-paper cursor-pointer">
                  {u.includes("11434") ? "Ollama" : "LM Studio"} · {u.replace("http://", "")}
                </button>
              ))}
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {error && (
          <motion.div initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: "auto" }} exit={{ opacity: 0, height: 0 }} className="rounded-xl border border-coral/25 bg-coral/[0.07] px-3.5 py-2.5 text-[13px] text-coral-soft">
            {error}
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {verified && models.length > 1 && (
          <motion.div initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: "auto" }} className="flex items-center justify-between gap-3 text-[13px]">
            <span className="text-mute">Model</span>
            <select value={model ?? ""} onChange={(e) => setModel(e.target.value)} className="h-9 max-w-[320px] rounded-lg bg-black/40 border border-white/[0.09] px-2.5 font-mono text-[12.5px] outline-none">
              {models.map((m) => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
          </motion.div>
        )}
      </AnimatePresence>

      {pre && !hasStt && (
        <div className="rounded-2xl border border-white/[0.07] bg-white/[0.025] p-4">
          <div className="mb-1 text-[14px] font-medium">Transcription</div>
          <p className="mb-3 text-[12.5px] leading-relaxed text-mute">
            {pre.name} picks the moments. Captions need word timings, which come from Whisper. Use a free Groq key for it, or run Whisper on this computer.
          </p>
          <Segmented
            value={sttChoice}
            onChange={setSttChoice}
            options={[
              { value: "groq", label: "Free Groq key" },
              { value: "local", label: "On this computer" },
            ]}
          />
          <div className="mt-3">
            {sttChoice === "groq" ? (
              <div className="flex items-center gap-2">
                <KeyInput value={groqKey} onChange={setGroqKey} placeholder={current?.stt_provider === "groq" ? "Groq key saved" : "Paste a Groq key (console.groq.com)"} />
                <Button tone="ghost" size="sm" onClick={() => openUrl("https://console.groq.com/keys")}>Get one <ArrowUpRight size={13} /></Button>
              </div>
            ) : whisperReady ? (
              <div className="flex items-center gap-2 text-[13px] text-mint"><Check size={15} /> On-device Whisper is installed.</div>
            ) : (
              <div className="flex items-center gap-3">
                <Segmented value={whisperSize} onChange={setWhisperSize} options={[{ value: "base", label: "Fast · 60 MB" }, { value: "small", label: "Accurate · 190 MB" }]} />
                <Button tone="soft" onClick={installWhisper} disabled={whisperBusy !== null || info?.platform !== "windows"} className="w-40">
                  {whisperBusy !== null ? <><Loader2 size={15} className="animate-spin" /> {Math.round(whisperBusy * 100)}%</> : <><HardDrive size={15} /> Install</>}
                </Button>
              </div>
            )}
          </div>
        </div>
      )}

      <div className="flex justify-end">
        <Button tone="primary" size="lg" onClick={save} disabled={!canSave || busy}>
          {busy && verified ? <Loader2 size={17} className="animate-spin" /> : null}
          Save and continue
        </Button>
      </div>
    </div>
  );
}
