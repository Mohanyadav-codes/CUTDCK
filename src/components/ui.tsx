import { AnimatePresence, motion } from "motion/react";
import { Check, Info, X } from "lucide-react";
import type { ButtonHTMLAttributes, ReactNode } from "react";
import { useStore } from "../lib/store";

type Tone = "primary" | "ghost" | "soft" | "danger";

export function Button({
  tone = "soft",
  size = "md",
  className = "",
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { tone?: Tone; size?: "sm" | "md" | "lg" }) {
  const tones: Record<Tone, string> = {
    primary:
      "bg-gradient-to-b from-[#ff6d62] to-coral text-white shadow-[0_8px_30px_-8px_rgba(255,90,78,0.65),inset_0_1px_0_rgba(255,255,255,0.35)] hover:brightness-110",
    soft: "bg-white/[0.06] hover:bg-white/[0.1] text-paper border border-white/[0.08]",
    ghost: "hover:bg-white/[0.06] text-mute hover:text-paper",
    danger: "bg-coral/10 hover:bg-coral/20 text-coral-soft border border-coral/20",
  };
  const sizes = { sm: "h-8 px-3 text-[13px] rounded-lg gap-1.5", md: "h-10 px-4 text-sm rounded-xl gap-2", lg: "h-12 px-6 text-[15px] rounded-2xl gap-2.5" };
  return (
    <button
      className={`inline-flex items-center justify-center font-medium transition-all duration-150 active:scale-[0.97] disabled:opacity-40 disabled:pointer-events-none cursor-pointer ${tones[tone]} ${sizes[size]} ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}

/** Circular progress ring. */
export function Ring({ value, size = 64, stroke = 4, className = "" }: { value: number; size?: number; stroke?: number; className?: string }) {
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  return (
    <svg width={size} height={size} className={className} style={{ transform: "rotate(-90deg)" }}>
      <defs>
        <linearGradient id={`ring-${size}`} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#FFB547" />
          <stop offset="1" stopColor="#FF5A4E" />
        </linearGradient>
      </defs>
      <circle cx={size / 2} cy={size / 2} r={r} stroke="rgba(255,255,255,0.09)" strokeWidth={stroke} fill="none" />
      <motion.circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        stroke={`url(#ring-${size})`}
        strokeWidth={stroke}
        strokeLinecap="round"
        fill="none"
        strokeDasharray={c}
        animate={{ strokeDashoffset: c * (1 - Math.max(0.02, Math.min(1, value))) }}
        transition={{ type: "spring", stiffness: 60, damping: 18 }}
      />
    </svg>
  );
}

export function Toasts({ compact = false, corner = "br" }: { compact?: boolean; corner?: string }) {
  const toasts = useStore((s) => s.toasts);
  const drop = useStore((s) => s.dropToast);
  return (
    <div
      className={`pointer-events-none fixed z-[100] flex flex-col gap-2 ${corner.endsWith("l") ? "items-start" : "items-end"}`}
      style={compact ? { [corner.endsWith("l") ? "left" : "right"]: 96, [corner.startsWith("t") ? "top" : "bottom"]: 24 } : { right: 24, bottom: 24 }}
    >
      <AnimatePresence>
        {toasts.map((t) => (
          <motion.div
            key={t.id}
            data-hit
            layout
            initial={{ opacity: 0, y: 16, scale: 0.95 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, x: 40, transition: { duration: 0.18 } }}
            className="glass-dark pointer-events-auto flex max-w-[360px] items-start gap-3 rounded-2xl px-4 py-3 text-[13px] leading-snug shadow-2xl"
          >
            <span className={`mt-0.5 ${t.tone === "error" ? "text-coral" : t.tone === "good" ? "text-mint" : "text-amber"}`}>
              {t.tone === "good" ? <Check size={15} /> : t.tone === "error" ? <X size={15} /> : <Info size={15} />}
            </span>
            <span className="flex-1 text-paper/90">{t.text}</span>
            <button className="text-dim hover:text-paper" onClick={() => drop(t.id)}>
              <X size={13} />
            </button>
          </motion.div>
        ))}
      </AnimatePresence>
    </div>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  disabled,
}: {
  value: T;
  options: { value: T; label: ReactNode; title?: string }[];
  onChange: (v: T) => void;
  disabled?: boolean;
}) {
  return (
    <div className={`relative flex rounded-xl bg-black/30 p-1 border border-white/[0.06] ${disabled ? "opacity-50 pointer-events-none" : ""}`}>
      {options.map((o) => (
        <button
          key={o.value}
          title={o.title}
          onClick={() => onChange(o.value)}
          className={`relative z-10 flex-1 h-8 px-2.5 text-[12.5px] font-medium rounded-lg transition-colors cursor-pointer ${value === o.value ? "text-paper" : "text-mute hover:text-paper"}`}
        >
          {value === o.value && (
            <motion.span layoutId={`seg-${options.map((x) => x.value).join("")}`} className="absolute inset-0 -z-10 rounded-lg bg-white/[0.1] border border-white/[0.08]" transition={{ type: "spring", stiffness: 500, damping: 36 }} />
          )}
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Toggle({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button onClick={() => onChange(!on)} className={`relative h-6 w-11 rounded-full transition-colors cursor-pointer ${on ? "bg-coral" : "bg-white/15"}`}>
      <motion.span className="absolute top-0.5 h-5 w-5 rounded-full bg-white shadow" animate={{ left: on ? 22 : 2 }} transition={{ type: "spring", stiffness: 600, damping: 32 }} />
    </button>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 py-3.5 border-b hairline last:border-0">
      <div className="min-w-0">
        <div className="text-[14px] text-paper">{label}</div>
        {hint && <div className="mt-0.5 text-[12.5px] text-mute leading-snug">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function ScoreDot({ score }: { score: number }) {
  const color = score >= 75 ? "#4BE3A0" : score >= 55 ? "#FFB547" : "#8a857d";
  return (
    <span className="inline-flex items-center gap-1 font-mono text-[11px] font-semibold" style={{ color }}>
      <span className="h-1.5 w-1.5 rounded-full" style={{ background: color, boxShadow: `0 0 8px ${color}` }} />
      {Math.round(score)}
    </span>
  );
}
