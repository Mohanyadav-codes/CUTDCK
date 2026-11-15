import { FolderOpen, Info, KeyRound } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { AiConnect } from "../components/AiConnect";
import { CaptionSample } from "../components/CaptionSample";
import { Button, Field, Segmented, Toggle } from "../components/ui";
import { api, errText } from "../lib/api";
import { useStore } from "../lib/store";
import type { Layout, Settings } from "../lib/types";

export function SettingsView() {
  const info = useStore((s) => s.info);
  const loadInfo = useStore((s) => s.loadInfo);
  const toast = useStore((s) => s.toast);
  const [editAi, setEditAi] = useState(false);
  if (!info) return null;
  const s = info.settings;
  const pre = info.presets.find((p) => p.id === s.ai?.provider);

  async function save(next: Settings) {
    try {
      await api.saveSettings(next);
      await loadInfo();
    } catch (e) {
      toast(errText(e), "error");
    }
  }
  const job = (patch: Partial<Settings["job"]>) => save({ ...s, job: { ...s.job, ...patch } });

  return (
    <div className="scroll-thin h-full overflow-y-auto">
      <div className="mx-auto max-w-[760px] px-10 pb-20 pt-10">
        <h1 className="font-display text-[40px] font-bold tracking-[-0.04em]">Settings</h1>

        <Section title="AI">
          {editAi ? (
            <AiConnect compact onSaved={() => { setEditAi(false); toast("AI settings saved", "good"); }} />
          ) : (
            <>
              <Field
                label={pre ? `${pre.name}${s.ai?.provider === "local" ? ` · ${s.ai.base_url}` : ""}` : "Not connected"}
                hint={
                  s.ai ? (
                    <span className="font-mono text-[12px]">
                      {s.ai.model} · captions via {s.ai.stt === "local" ? "on-device Whisper" : s.ai.stt_provider === "groq" || s.ai.provider === "groq" ? "Groq Whisper" : "cloud Whisper"}
                    </span>
                  ) : undefined
                }
              >
                <Button size="sm" onClick={() => setEditAi(true)}><KeyRound size={13} /> Change</Button>
              </Field>
            </>
          )}
        </Section>

        <Section title="Clips">
          <Field label="Clips per video" hint="The best moments, ranked.">
            <Segmented value={String(s.job.max_clips)} onChange={(v) => job({ max_clips: Number(v) })} options={["4", "6", "8", "12", "16"].map((v) => ({ value: v, label: v }))} />
          </Field>
          <Field label="Clip length" hint="Shorts and Reels work best under a minute.">
            <Segmented
              value={`${s.job.min_len}-${s.job.max_len}`}
              onChange={(v) => {
                const [a, b] = v.split("-").map(Number);
                job({ min_len: a, max_len: b });
              }}
              options={[
                { value: "15-35", label: "15-35 s" },
                { value: "15-60", label: "15-60 s" },
                { value: "30-90", label: "30-90 s" },
              ]}
            />
          </Field>
          <Field label="Default framing">
            <Segmented
              value={s.job.layout}
              onChange={(v: Layout) => job({ layout: v })}
              options={[
                { value: "auto", label: "Auto" },
                { value: "face", label: "Face" },
                { value: "split", label: "Split" },
                { value: "fit", label: "Fit" },
              ]}
            />
          </Field>
          <Field label="Cut silences and filler words" hint="Removes dead air and “um”s. Loud moments without talking are kept.">
            <Toggle on={s.job.remove_silence} onChange={(v) => job({ remove_silence: v })} />
          </Field>
          <Field label="Hook banner" hint="A title card for the first 3 seconds.">
            <Toggle on={s.job.hook_banner} onChange={(v) => job({ hook_banner: v })} />
          </Field>
          <div className="pt-4">
            <div className="mb-3 text-[14px]">Default caption style</div>
            <div className="grid grid-cols-5 gap-2">
              {info.styles.map((st) => (
                <button
                  key={st.id}
                  onClick={() => job({ caption_style: st.id })}
                  className={`flex h-20 flex-col items-center justify-center gap-2 rounded-xl border transition cursor-pointer ${s.job.caption_style === st.id ? "border-coral/60 bg-coral/[0.08]" : "border-white/[0.07] bg-black/30 hover:bg-white/[0.04]"}`}
                >
                  <CaptionSample style={st} scale={0.22} words={["SO", "GOOD"]} active={1} />
                  <span className="text-[11px] text-mute">{st.name}</span>
                </button>
              ))}
            </div>
          </div>
        </Section>

        <Section title="Long streams">
          <Field label="Listen to at most" hint="For long streams Cutdeck listens to the liveliest stretches only. Keeps it fast and inside free-tier limits.">
            <Segmented value={String(s.listen_budget_min)} onChange={(v) => save({ ...s, listen_budget_min: Number(v) })} options={[{ value: "30", label: "30 min" }, { value: "60", label: "1 h" }, { value: "120", label: "2 h" }]} />
          </Field>
        </Section>

        <Section title="App">
          <Field label="Export folder" hint={<span className="font-mono text-[12px]">{s.export_dir}</span>}>
            <Button
              size="sm"
              onClick={async () => {
                const d = await openDialog({ directory: true, defaultPath: s.export_dir });
                if (typeof d === "string") save({ ...s, export_dir: d });
              }}
            >
              <FolderOpen size={13} /> Change
            </Button>
          </Field>
          <Field label="Corner button position" hint="You can also drag the button to any corner.">
            <Segmented
              value={s.tray_corner || "br"}
              onChange={async (v) => {
                await api.setTrayCorner(v);
                await loadInfo();
              }}
              options={[
                { value: "tl", label: "? Top left" },
                { value: "tr", label: "? Top right" },
                { value: "bl", label: "? Bottom left" },
                { value: "br", label: "? Bottom right" },
              ]}
            />
          </Field>
          <Field label="Start with my computer" hint="Keeps the deck in the corner, ready for links.">
            <Toggle on={s.launch_at_login} onChange={(v) => save({ ...s, launch_at_login: v })} />
          </Field>
        </Section>

        <Section title="About">
          <div className="flex items-start gap-3 py-3 text-[13px] leading-relaxed text-mute">
            <Info size={16} className="mt-0.5 shrink-0" />
            <div>
              Cutdeck {info.version} by{" "}
              <button className="text-coral-soft hover:text-coral cursor-pointer" onClick={() => openUrl("https://mohanbuilds.com")}>mohanbuilds</button>. Videos are processed on this
              computer; only audio goes to your AI provider for transcription. Uses FFmpeg (GPL), yt-dlp, Deno and whisper.cpp as separate programs. Only clip content you have the right to use.
            </div>
          </div>
        </Section>
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mt-10">
      <h2 className="mb-2 text-[12px] font-semibold uppercase tracking-[0.12em] text-dim">{title}</h2>
      <div className="glass rounded-2xl px-5 py-1">{children}</div>
    </section>
  );
}
