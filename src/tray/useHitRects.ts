import { useEffect } from "react";
import { api, type Rect } from "../lib/api";

/**
 * Tell the backend which parts of the transparent tray window are solid, so
 * clicks pass through everywhere else. Elements opt in with `data-hit`.
 */
export function useHitRects(corner: string = "br") {
  useEffect(() => {
    let last = "";
    let alive = true;
    const tick = () => {
      if (!alive) return;
      const rects: Rect[] = [];
      document.querySelectorAll<HTMLElement>("[data-hit]").forEach((el) => {
        const r = el.getBoundingClientRect();
        if (r.width > 0 && r.height > 0) rects.push({ x: r.left - 4, y: r.top - 4, w: r.width + 8, h: r.height + 8 });
      });
      const W = window.innerWidth;
      const H = window.innerHeight;
      const zone: Rect = { x: corner.endsWith("l") ? 0 : W - 300, y: corner.startsWith("t") ? 0 : H - 300, w: 300, h: 300 };
      const key = corner + JSON.stringify(rects.map((r) => [Math.round(r.x), Math.round(r.y), Math.round(r.w), Math.round(r.h)]));
      if (key !== last) {
        last = key;
        api.setHitRects(rects, zone).catch(() => {});
      }
      setTimeout(() => requestAnimationFrame(tick), 60);
    };
    tick();
    return () => {
      alive = false;
    };
  }, [corner]);
}
