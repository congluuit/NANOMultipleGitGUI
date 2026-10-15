import { useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronRight, Copy, FileDiff, Minus, Plus, Undo2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { splitPath } from "@/features/commit/format";
import type { StatusEntry } from "@/ipc";
import { formatBytes, LARGE_FILE_WARN_BYTES } from "@/lib/format";
import { cn } from "@/lib/utils";
import { LfsLockBadge } from "@/features/lfs/LfsLockBadge";
import { clickSelect, KIND_CLASS, kindLetter } from "./format";

const MAX_ROWS = 2000;

export type SectionKind = "unstaged" | "staged" | "conflicted";

interface Props {
  kind: SectionKind;
  entries: StatusEntry[];
  /** Path of the file whose diff is open in this section, if any. */
  activePath: string | null;
  onOpen: (entry: StatusEntry) => void;
  /** Stage (unstaged/conflicted) or unstage (staged). */
  onPrimary: (entries: StatusEntry[]) => void;
  onPrimaryAll: () => void;
  onDiscard?: (entries: StatusEntry[]) => void;
  /** Extra context-menu items for the right-clicked files (file history, blame…). */
  menuExtra?: (entries: StatusEntry[]) => React.ReactNode;
}

export function FileSection({
  kind,
  entries,
  activePath,
  onOpen,
  onPrimary,
  onPrimaryAll,
  onDiscard,
  menuExtra,
}: Props) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(true);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const anchor = useRef<string | null>(null);
  const [menuTarget, setMenuTarget] = useState<StatusEntry[]>([]);
  const order = useMemo(() => entries.map((e) => e.path), [entries]);
  const byPath = useMemo(() => new Map(entries.map((e) => [e.path, e])), [entries]);

  const title =
    kind === "staged"
      ? t("wip.staged")
      : kind === "conflicted"
        ? t("wip.conflicts")
        : t("wip.unstaged");
  const primaryLabel =
    kind === "staged"
      ? t("wip.unstage")
      : kind === "conflicted"
        ? t("wip.markResolved")
        : t("wip.stage");
  const PrimaryIcon = kind === "staged" ? Minus : Plus;
  // Row buttons are short words (the full action stays in the tooltip).
  const primaryShort =
    kind === "staged"
      ? t("wip.unstage")
      : kind === "conflicted"
        ? t("wip.resolvedShort")
        : t("wip.stage");

  const targetFor = (entry: StatusEntry) =>
    selected.has(entry.path)
      ? order.filter((p) => selected.has(p)).flatMap((p) => byPath.get(p) ?? [])
      : [entry];

  if (entries.length === 0 && kind !== "unstaged" && kind !== "staged") return null;

  return (
    <section className="border-b">
      <div className="flex h-7 items-center gap-1 bg-surface-2/60 pr-1 pl-1.5 text-[11px] font-semibold text-muted-foreground uppercase">
        <button
          type="button"
          onClick={() => setOpen(!open)}
          className="flex min-w-0 flex-1 items-center gap-1 text-left"
        >
          {open ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
          <span className={cn("truncate", kind === "conflicted" && "text-danger")}>{title}</span>
          <span className="rounded bg-background px-1 font-mono normal-case">{entries.length}</span>
        </button>
        {entries.length > 0 && kind !== "conflicted" && (
          <button
            type="button"
            onClick={onPrimaryAll}
            className={cn(
              "flex shrink-0 items-center gap-1 rounded border bg-background px-1.5 py-0.5 text-[11px] font-medium text-foreground normal-case",
              kind === "staged"
                ? "hover:bg-surface-2"
                : "border-primary/40 text-primary hover:bg-primary/10",
            )}
          >
            <PrimaryIcon className="size-3" />
            {kind === "staged" ? t("wip.unstageAll") : t("wip.stageAll")}
          </button>
        )}
      </div>
      {open && entries.length === 0 && (
        <p className="px-3 py-2 text-xs text-muted-foreground">
          {kind === "staged" ? t("wip.noStaged") : t("wip.noUnstaged")}
        </p>
      )}
      {open && entries.length > 0 && (
        <ContextMenu>
          <ContextMenuTrigger asChild>
            <ul
              role="listbox"
              aria-label={title}
              aria-multiselectable
              onContextMenu={(e) => {
                const el = (e.target as HTMLElement).closest<HTMLElement>("[data-path]");
                const entry = el ? byPath.get(el.dataset.path!) : undefined;
                if (!entry) {
                  e.preventDefault();
                  return;
                }
                if (!selected.has(entry.path)) {
                  setSelected(new Set([entry.path]));
                  anchor.current = entry.path;
                }
                setMenuTarget(targetFor(entry));
              }}
            >
              {entries.slice(0, MAX_ROWS).map((e) => (
                <FileRow
                  key={`${e.oldPath ?? ""}>${e.path}`}
                  entry={e}
                  active={e.path === activePath}
                  selected={selected.has(e.path)}
                  primaryLabel={primaryLabel}
                  primaryShort={primaryShort}
                  discardLabel={onDiscard ? t("wip.discard") : null}
                  onClick={(ev) => {
                    const mods = { ctrl: ev.ctrlKey || ev.metaKey, shift: ev.shiftKey };
                    setSelected(clickSelect(order, selected, anchor.current, e.path, mods));
                    if (!mods.shift) anchor.current = e.path;
                    if (!mods.ctrl && !mods.shift) onOpen(e);
                  }}
                  onPrimary={() => onPrimary(targetFor(e))}
                  onDiscard={() => onDiscard?.(targetFor(e))}
                />
              ))}
              {entries.length > MAX_ROWS && (
                <li className="px-3 py-1 text-xs text-muted-foreground">
                  {t("wip.more", { count: entries.length - MAX_ROWS })}
                </li>
              )}
            </ul>
          </ContextMenuTrigger>
          <ContextMenuContent>
            {menuTarget.length > 1 && (
              <ContextMenuLabel>{t("wip.selected", { count: menuTarget.length })}</ContextMenuLabel>
            )}
            <ContextMenuItem onSelect={() => onPrimary(menuTarget)}>
              <PrimaryIcon />
              {primaryLabel}
            </ContextMenuItem>
            {onDiscard && (
              <ContextMenuItem variant="destructive" onSelect={() => onDiscard(menuTarget)}>
                <Undo2 />
                {t("wip.discard")}
              </ContextMenuItem>
            )}
            <ContextMenuSeparator />
            <ContextMenuItem
              disabled={menuTarget.length !== 1}
              onSelect={() => menuTarget[0] && onOpen(menuTarget[0])}
            >
              <FileDiff />
              {t("wip.openDiff")}
            </ContextMenuItem>
            <ContextMenuItem
              onSelect={() =>
                void navigator.clipboard.writeText(menuTarget.map((e) => e.path).join("\n"))
              }
            >
              <Copy />
              {t("wip.copyPath")}
            </ContextMenuItem>
            {menuExtra && (
              <>
                <ContextMenuSeparator />
                {menuExtra(menuTarget)}
              </>
            )}
          </ContextMenuContent>
        </ContextMenu>
      )}
    </section>
  );
}

interface RowProps {
  entry: StatusEntry;
  active: boolean;
  selected: boolean;
  primaryLabel: string;
  primaryShort: string;
  discardLabel: string | null;
  onClick: (e: React.MouseEvent) => void;
  onPrimary: () => void;
  onDiscard: () => void;
}

function FileRow({
  entry,
  active,
  selected,
  primaryLabel,
  primaryShort,
  discardLabel,
  onClick,
  onPrimary,
  onDiscard,
}: RowProps) {
  const { t } = useTranslation();
  const { dir, name } = splitPath(entry.path);
  const large = entry.size !== null && entry.size >= LARGE_FILE_WARN_BYTES;
  return (
    <li
      role="option"
      aria-selected={selected}
      data-path={entry.path}
      onClick={onClick}
      title={entry.oldPath ? `${entry.oldPath} → ${entry.path}` : entry.path}
      className={cn(
        "group flex h-6 cursor-default items-center gap-2 pr-1 pl-3 text-xs select-none hover:bg-surface-2",
        selected && "bg-primary/10 hover:bg-primary/15",
        active && "shadow-[inset_2px_0_0_var(--primary)]",
      )}
    >
      <span
        className={cn("w-3 shrink-0 font-mono font-bold", KIND_CLASS[entry.kind])}
        title={t(`wip.kind.${entry.kind}`)}
      >
        {kindLetter(entry.kind)}
      </span>
      <span className="min-w-0 flex-1 truncate">
        <span className="text-muted-foreground">{dir}</span>
        {name}
      </span>
      <LfsLockBadge path={entry.path} />
      {large && (
        <span
          className="shrink-0 rounded bg-warning/15 px-1 text-[10px] font-semibold text-warning"
          title={t("wip.largeTitle", { size: formatBytes(entry.size ?? 0) })}
        >
          {t("wip.large")} · {formatBytes(entry.size ?? 0)}
        </span>
      )}
      <span className="hidden shrink-0 items-center gap-0.5 group-hover:flex">
        {discardLabel && (
          <button
            type="button"
            title={discardLabel}
            aria-label={discardLabel}
            onClick={(e) => {
              e.stopPropagation();
              onDiscard();
            }}
            className="rounded px-1.5 py-px text-[11px] font-medium text-muted-foreground hover:bg-background hover:text-danger"
          >
            {t("wip.undoShort")}
          </button>
        )}
        <button
          type="button"
          title={primaryLabel}
          aria-label={primaryLabel}
          onClick={(e) => {
            e.stopPropagation();
            onPrimary();
          }}
          className="rounded border bg-background px-1.5 py-px text-[11px] font-medium text-foreground hover:border-primary/50 hover:text-primary"
        >
          {primaryShort}
        </button>
      </span>
    </li>
  );
}
