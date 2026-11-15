import { motion } from "motion/react";
import { CaptionSample } from "./CaptionSample";
import type { CaptionStyle } from "../lib/types";

const SCENES = [
  { bg: "radial-gradient(120% 80% at 30% 20%, #3b2a8f, #120d2e 60%, #07060f)", words: ["NO", "WAY", "BRO"], tag: "Gaming" },
  { bg: "radial-gradient(120% 80% at 70% 30%, #a0442a, #2b1410 60%, #0c0706)", words: ["HE", "SAID", "WHAT"], tag: "Podcast" },
  { bg: "radial-gradient(120% 90% at 50% 10%, #ffb547, #c2471f 45%, #1d0b06)", words: ["$10,000", "CHALLENGE"], tag: "Challenge" },
  { bg: "radial-gradient(120% 80% at 40% 30%, #0f6b6b, #08231f 60%, #040a09)", words: ["CLUTCH", "1V4"], tag: "Stream" },
  { bg: "radial-gradient(120% 80% at 60% 25%, #6b2350, #1e0a18 60%, #08040a)", words: ["WATCH", "THIS"], tag: "Vlog" },
];

/** The fanned deck of sample shorts on the welcome screen. */
export function DeckHero({ styles }: { styles: CaptionStyle[] }) {
  const n = SCENES.length;
  return (
    <div className="relative h-[280px] w-[720px]">
      {SCENES.map((s, i) => {
        const mid = (n - 1) / 2;
        const off = i - mid;
        const style = styles[i % Math.max(1, styles.length)];
        return (
          <motion.div
            key={i}
            className="absolute left-1/2 top-1/2 h-[250px] w-[141px] overflow-hidden rounded-[22px] border border-white/10 shadow-[0_30px_80px_-20px_rgba(0,0,0,0.9)]"
            style={{ background: s.bg, marginLeft: -70, marginTop: -125, zIndex: 10 - Math.abs(off) }}
            initial={{ rotate: 0, x: 0, y: 40, opacity: 0, scale: 0.9 }}
            animate={{ rotate: off * 9, x: off * 100, y: Math.abs(off) * 18, opacity: 1, scale: 1 - Math.abs(off) * 0.04 }}
            transition={{ type: "spring", stiffness: 120, damping: 16, delay: 0.15 + Math.abs(off) * 0.08 }}
            whileHover={{ y: Math.abs(off) * 22 - 18, scale: 1.04, transition: { type: "spring", stiffness: 300, damping: 20 } }}
          >
            <div className="absolute inset-0 opacity-30" style={{ background: "repeating-linear-gradient(0deg, rgba(255,255,255,0.04) 0 2px, transparent 2px 4px)" }} />
            <div className="absolute left-1/2 top-[34%] h-20 w-20 -translate-x-1/2 rounded-full bg-white/10 blur-xl" />
            <div className="absolute left-3 top-3 rounded-full bg-black/40 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-white/80 backdrop-blur">{s.tag}</div>
            {style && (
              <div className="absolute inset-x-3 top-[60%] flex justify-center text-center [&>div]:whitespace-normal">
                <CaptionSample style={style} words={s.words} active={s.words.length - 1} scale={0.15} />
              </div>
            )}
            <div className="absolute inset-x-3 bottom-3 h-1 overflow-hidden rounded-full bg-white/15">
              <motion.div className="h-full bg-white/80" initial={{ width: "0%" }} animate={{ width: "100%" }} transition={{ duration: 5 + i, repeat: Infinity, ease: "linear" }} />
            </div>
          </motion.div>
        );
      })}
    </div>
  );
}
