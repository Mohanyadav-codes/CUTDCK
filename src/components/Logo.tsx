import { motion } from "motion/react";

/** The Cutdeck mark: a fanned deck of three cards, the front one sliced. */
export function Mark({ size = 28, animate = false }: { size?: number; animate?: boolean }) {
  const card = (rot: number, fill: string, delay: number, x = 0) => (
    <motion.rect
      x={14 + x}
      y={6}
      width={20}
      height={34}
      rx={4.5}
      fill={fill}
      style={{ originX: "24px", originY: "40px" }}
      initial={animate ? { rotate: 0, opacity: 0 } : false}
      animate={{ rotate: rot, opacity: 1 }}
      transition={{ type: "spring", stiffness: 260, damping: 18, delay }}
    />
  );
  return (
    <svg width={size} height={size} viewBox="0 0 48 48" fill="none" aria-hidden>
      <defs>
        <linearGradient id="cd-front" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#FF8A7F" />
          <stop offset="1" stopColor="#FF5A4E" />
        </linearGradient>
      </defs>
      {card(-18, "#3a3a48", 0.0)}
      {card(-7, "#FFB547", 0.06)}
      {card(6, "url(#cd-front)", 0.12)}
      <motion.path
        d="M19 30 L33 18"
        stroke="#0b0b10"
        strokeWidth={2.6}
        strokeLinecap="round"
        style={{ originX: "24px", originY: "40px", rotate: 6 }}
        initial={animate ? { pathLength: 0 } : false}
        animate={{ pathLength: 1 }}
        transition={{ delay: 0.45, duration: 0.35, ease: "easeOut" }}
      />
    </svg>
  );
}

export function Wordmark({ size = 22 }: { size?: number }) {
  return (
    <div className="flex items-center gap-2 select-none">
      <Mark size={size + 8} />
      <span className="font-display font-bold tracking-tight" style={{ fontSize: size, letterSpacing: "-0.03em" }}>
        cutdeck
      </span>
    </div>
  );
}
