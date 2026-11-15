import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";
import type { ReactNode } from "react";

export function TitleBar({ children }: { children?: ReactNode }) {
  const w = getCurrentWindow();
  const btn = "grid h-full w-12 place-items-center text-mute hover:text-paper hover:bg-white/[0.06] transition-colors cursor-pointer";
  return (
    <div data-tauri-drag-region className="relative z-50 flex h-11 shrink-0 items-center select-none" onDoubleClick={() => w.toggleMaximize()}>
      <div data-tauri-drag-region className="flex h-full flex-1 items-center gap-3 pl-4">
        {children}
      </div>
      <div className="flex h-full">
        <button className={btn} onClick={() => w.minimize()} aria-label="Minimize"><Minus size={15} /></button>
        <button className={btn} onClick={() => w.toggleMaximize()} aria-label="Maximize"><Square size={12} /></button>
        <button className={`${btn} hover:!bg-coral hover:!text-white`} onClick={() => w.hide()} aria-label="Close"><X size={16} /></button>
      </div>
    </div>
  );
}
