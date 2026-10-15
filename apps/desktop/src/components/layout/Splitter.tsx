import { useRef, type KeyboardEvent, type PointerEvent } from "react";
import { cn } from "@/lib/utils";

const STEP = 16;
const BIG_STEP = 64;

/**
 * Vertical drag handle between two panels, sizing the panel on its `side`.
 *
 * Pointer events (not HTML5 drag and drop, which Tauri swallows on Windows) with pointer
 * capture; ←/→ (Shift = bigger steps), Home/End, Escape cancels a drag, double-click resets.
 */
export function Splitter({
  value,
  min,
  max,
  defaultValue,
  side,
  label,
  controls,
  onChange,
  className,
}: {
  value: number;
  min: number;
  max: number;
  defaultValue: number;
  /** Which neighbor the handle sizes: dragging toward the middle grows it. */
  side: "left" | "right";
  label: string;
  /** id of the sized panel (`aria-controls`). */
  controls?: string;
  /** `final` is false while dragging and true once the width settles (save it then). */
  onChange: (px: number, final: boolean) => void;
  className?: string;
}) {
  const drag = useRef<{ x: number; start: number; last: number } | null>(null);
  const clamp = (px: number) => Math.round(Math.min(max, Math.max(min, px)));
  const sign = side === "left" ? 1 : -1;

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.focus();
    e.currentTarget.setPointerCapture?.(e.pointerId);
    drag.current = { x: e.clientX, start: value, last: value };
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const next = clamp(d.start + sign * (e.clientX - d.x));
    if (next !== d.last) {
      d.last = next;
      onChange(next, false);
    }
  };
  const end = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    drag.current = null;
    e.currentTarget.releasePointerCapture?.(e.pointerId);
    onChange(d.last, true);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? BIG_STEP : STEP;
    let next: number | null = null;
    if (e.key === "ArrowLeft") next = value - sign * step;
    else if (e.key === "ArrowRight") next = value + sign * step;
    else if (e.key === "Home") next = min;
    else if (e.key === "End") next = max;
    else if (e.key === "Escape" && drag.current) {
      const start = drag.current.start;
      drag.current = null;
      onChange(start, true);
      e.preventDefault();
      return;
    }
    if (next === null) return;
    e.preventDefault();
    onChange(clamp(next), true);
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-controls={controls}
      aria-valuenow={value}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      title={label}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={end}
      onPointerCancel={end}
      onKeyDown={onKeyDown}
      onDoubleClick={() => onChange(clamp(defaultValue), true)}
      className={cn(
        // 1px line with a wider invisible hit area; above both neighbors.
        "relative z-10 w-px shrink-0 cursor-col-resize touch-none bg-border outline-none select-none",
        "before:absolute before:inset-y-0 before:-right-1 before:-left-1 before:content-['']",
        "hover:bg-primary/60 focus-visible:bg-primary active:bg-primary",
        className,
      )}
    />
  );
}
