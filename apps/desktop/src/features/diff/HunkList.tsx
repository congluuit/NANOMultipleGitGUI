import { useMemo, useRef } from "react";
import { Minus, Plus, Undo2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { DiffRequest, Hunk, LineSelection, PatchAction } from "@/ipc";
import { cn } from "@/lib/utils";
import { isSelected, selectRange, toggleLine, wholeHunk, type LineSel } from "./selection";

/** Rendering every line of a huge diff as DOM would freeze the view. */
const MAX_LINES = 8000;

interface Props {
  hunks: Hunk[];
  selectable: boolean;
  selection: LineSel;
  onSelection: (sel: LineSel) => void;
  side: DiffRequest["side"];
  onHunk: (action: PatchAction, selection: LineSelection[]) => void;
}

const SIGN = { add: "+", delete: "−", context: " " } as const;

/** How many lines of each hunk fit in `max` lines overall. */
function limitLines(hunks: Hunk[], max: number): { visible: number[]; truncated: boolean } {
  let budget = max;
  const visible = hunks.map((h) => {
    const n = Math.min(h.lines.length, Math.max(0, budget));
    budget -= n;
    return n;
  });
  return { visible, truncated: visible.some((n, i) => n < (hunks[i]?.lines.length ?? 0)) };
}

export function HunkList({ hunks, selectable, selection, onSelection, side, onHunk }: Props) {
  const { t } = useTranslation();
  const anchor = useRef<{ hunk: number; line: number } | null>(null);
  const { visible, truncated } = useMemo(() => limitLines(hunks, MAX_LINES), [hunks]);

  return (
    <div className="min-h-0 flex-1 overflow-auto font-mono text-[12px] leading-[18px]">
      {selectable && (
        <p className="sticky left-0 px-3 py-1 font-sans text-[11px] text-muted-foreground">
          {t("diff.selectHint")}
        </p>
      )}
      {hunks.map((h, hi) => {
        const count = visible[hi] ?? 0;
        if (count === 0) return null;
        const shown = count < h.lines.length ? h.lines.slice(0, count) : h.lines;
        return (
          <div key={hi} className="mb-2 min-w-max">
            <div className="sticky top-0 z-10 flex items-center gap-2 border-y bg-surface-2 px-3 py-0.5 font-sans text-[11px] text-muted-foreground">
              <span className="font-mono">{h.header}</span>
              <span className="flex-1" />
              {selectable && side === "unstaged" && (
                <>
                  <HunkButton
                    label={t("diff.discardHunk")}
                    onClick={() => onHunk("discard", wholeHunk(hunks, hi))}
                  >
                    <Undo2 className="size-3" />
                  </HunkButton>
                  <HunkButton
                    label={t("diff.stageHunk")}
                    primary
                    onClick={() => onHunk("stage", wholeHunk(hunks, hi))}
                  >
                    <Plus className="size-3" />
                  </HunkButton>
                </>
              )}
              {selectable && side === "staged" && (
                <HunkButton
                  label={t("diff.unstageHunk")}
                  primary
                  onClick={() => onHunk("unstage", wholeHunk(hunks, hi))}
                >
                  <Minus className="size-3" />
                </HunkButton>
              )}
            </div>
            {shown.map((l, li) => {
              const change = l.kind !== "context";
              const sel = change && isSelected(selection, hi, li);
              return (
                <div
                  key={li}
                  data-kind={l.kind}
                  aria-selected={selectable && change ? sel : undefined}
                  onClick={(e) => {
                    if (!selectable || !change) return;
                    const a = anchor.current;
                    if (e.shiftKey && a && a.hunk === hi) {
                      onSelection(selectRange(selection, hunks, hi, a.line, li));
                    } else {
                      onSelection(toggleLine(selection, hunks, hi, li));
                    }
                    anchor.current = { hunk: hi, line: li };
                  }}
                  className={cn(
                    "flex select-none",
                    l.kind === "add" && "bg-success/10",
                    l.kind === "delete" && "bg-danger/10",
                    selectable && change && "cursor-pointer hover:brightness-95",
                    sel && "bg-primary/20 shadow-[inset_3px_0_0_var(--primary)]",
                  )}
                >
                  <span className="w-12 shrink-0 pr-2 text-right text-muted-foreground/70 tabular-nums">
                    {l.oldLine ?? ""}
                  </span>
                  <span className="w-12 shrink-0 pr-2 text-right text-muted-foreground/70 tabular-nums">
                    {l.newLine ?? ""}
                  </span>
                  <span
                    className={cn(
                      "w-5 shrink-0 text-center",
                      l.kind === "add" && "text-success",
                      l.kind === "delete" && "text-danger",
                    )}
                  >
                    {SIGN[l.kind]}
                  </span>
                  <span className="pr-4 whitespace-pre [tab-size:4]">
                    {l.text}
                    {l.noNewline && (
                      <span
                        className="ml-2 font-sans text-[10px] text-muted-foreground"
                        title={t("diff.noNewline")}
                      >
                        ⏎̸
                      </span>
                    )}
                  </span>
                </div>
              );
            })}
          </div>
        );
      })}
      {truncated && (
        <p className="px-3 py-2 font-sans text-xs text-warning">
          {t("diff.truncated", { count: MAX_LINES })}
        </p>
      )}
    </div>
  );
}

function HunkButton({
  label,
  primary,
  onClick,
  children,
}: {
  label: string;
  primary?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex items-center gap-1 rounded border px-1.5 py-px text-[11px]",
        primary
          ? "border-primary bg-primary text-primary-foreground hover:bg-primary/90"
          : "bg-background hover:bg-surface hover:text-foreground",
      )}
    >
      {children}
      {label}
    </button>
  );
}
