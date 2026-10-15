import type { ReactNode } from "react";
import { contrastText } from "@/features/graph/avatars";
import { cn } from "@/lib/utils";

/** Solid tag label; the text is black or white, whichever reads better on the color. */
export function TagPill({
  color,
  children,
  className,
  muted,
}: {
  color: string;
  children: ReactNode;
  className?: string;
  /** Outline only (tag not chosen in a picker). */
  muted?: boolean;
}) {
  return (
    <span
      className={cn(
        "inline-flex max-w-40 items-center gap-1 truncate rounded-full border px-1.5 text-[10px] leading-4 font-medium",
        className,
      )}
      style={
        muted
          ? { borderColor: color, color: "var(--text)" }
          : { background: color, borderColor: color, color: contrastText(color) }
      }
    >
      {children}
    </span>
  );
}
