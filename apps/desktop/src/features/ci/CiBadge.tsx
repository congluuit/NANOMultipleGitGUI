import type { CiJob, CiState } from "@/ipc";
import { cn } from "@/lib/utils";
import { STATE_ICONS, useCiTooltip } from "./hooks";
import { ciTextClass } from "./model";

/**
 * ✓ / ✗ / ● / – icon of a CI state in the success / danger / warning token colour, with a
 * tooltip. Renders nothing for `"none"`. `label` adds a text after the icon (e.g. `CI`).
 */
export function CiBadge({
  state,
  jobs,
  label,
  className,
  size = 14,
  title,
}: {
  state: CiState;
  jobs?: readonly CiJob[];
  label?: string;
  className?: string;
  size?: number;
  /** Overrides the tooltip. */
  title?: string;
}) {
  const tooltip = useCiTooltip();
  const Icon = STATE_ICONS[state];
  if (!Icon) return null;
  const text = title ?? tooltip(state, jobs);
  return (
    <span
      className={cn("inline-flex shrink-0 items-center gap-1", ciTextClass(state), className)}
      title={text}
      role="img"
      aria-label={text}
      data-ci-state={state}
    >
      <Icon
        style={{ width: size, height: size }}
        className={cn(state === "pending" && "animate-pulse")}
      />
      {label && <span className="font-medium">{label}</span>}
    </span>
  );
}
