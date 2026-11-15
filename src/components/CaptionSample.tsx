import type { CaptionStyle } from "../lib/types";

/** A live sample of a caption style, drawn with the same font as the export. */
export function CaptionSample({ style, words = ["THIS", "IS", "INSANE"], active = 2, scale = 0.32 }: { style: CaptionStyle; words?: string[]; active?: number; scale?: number }) {
  const size = style.size * scale;
  const stroke = style.outline_w * scale * 1.6;
  return (
    <div
      className="leading-none whitespace-nowrap"
      style={{
        fontFamily: `"${style.font}"`,
        fontWeight: style.font === "Poppins" ? 700 : 400,
        fontSize: size,
        letterSpacing: style.font === "Bangers" ? "0.02em" : "0",
        textShadow: style.shadow > 0 ? `0 ${2 * scale * 4}px ${6 * scale * 4}px rgba(0,0,0,0.55)` : undefined,
      }}
    >
      {words.map((w, i) => (
        <span
          key={i}
          className="inline-block"
          style={{
            color: i === active ? style.highlight : style.color,
            WebkitTextStroke: stroke > 0 ? `${stroke}px ${style.outline}` : undefined,
            paintOrder: "stroke fill",
            transform: i === active && style.anim === "pop" ? "scale(1.1)" : undefined,
            filter: i === active && style.anim === "glow" ? `drop-shadow(0 0 ${6 * scale * 3}px ${style.highlight})` : undefined,
            marginRight: i < words.length - 1 ? size * 0.28 : 0,
          }}
        >
          {style.upper ? w.toUpperCase() : w.charAt(0) + w.slice(1).toLowerCase()}
        </span>
      ))}
    </div>
  );
}
