import { motion } from "motion/react";

/** Slow-moving warm light behind the Studio. */
export function Backdrop({ intensity = 1 }: { intensity?: number }) {
  return (
    <div className="grain pointer-events-none absolute inset-0 overflow-hidden">
      <motion.div
        className="absolute -left-[15%] -top-[25%] h-[70vh] w-[70vh] rounded-full blur-[120px]"
        style={{ background: "radial-gradient(circle, rgba(255,90,78,0.55), transparent 65%)", opacity: 0.32 * intensity }}
        animate={{ x: [0, 60, -20, 0], y: [0, 40, 80, 0] }}
        transition={{ duration: 26, repeat: Infinity, ease: "easeInOut" }}
      />
      <motion.div
        className="absolute -right-[10%] top-[30%] h-[60vh] w-[60vh] rounded-full blur-[130px]"
        style={{ background: "radial-gradient(circle, rgba(255,181,71,0.5), transparent 65%)", opacity: 0.18 * intensity }}
        animate={{ x: [0, -50, 30, 0], y: [0, -60, 20, 0] }}
        transition={{ duration: 32, repeat: Infinity, ease: "easeInOut" }}
      />
      <motion.div
        className="absolute bottom-[-30%] left-[30%] h-[55vh] w-[55vh] rounded-full blur-[140px]"
        style={{ background: "radial-gradient(circle, rgba(88,120,255,0.4), transparent 65%)", opacity: 0.12 * intensity }}
        animate={{ x: [0, 80, -40, 0] }}
        transition={{ duration: 38, repeat: Infinity, ease: "easeInOut" }}
      />
      <div className="absolute inset-0" style={{ background: "radial-gradient(ellipse at 50% 120%, transparent 40%, rgba(7,7,10,0.85))" }} />
    </div>
  );
}
