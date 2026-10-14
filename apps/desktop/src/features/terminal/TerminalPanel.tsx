import { ChevronDown, Plus, SquareTerminal, X } from "lucide-react";
import type React from "react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { useRepoAccount } from "@/features/accounts/switcher";
import { MAX_HEIGHT, MIN_HEIGHT, useTerminal } from "./store";
import TerminalView from "./TerminalView";

/** Largest panel height that still leaves room for the graph above it. */
const viewportLimit = () => Math.max(MIN_HEIGHT, Math.round(window.innerHeight * 0.7));

function ResizeHandle() {
  const { t } = useTranslation();
  const height = useTerminal((s) => s.height);
  const setHeight = useTerminal((s) => s.setHeight);
  const resize = (next: number) => setHeight(Math.min(next, viewportLimit()));

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    const handle = e.currentTarget;
    const startY = e.clientY;
    const startHeight = handle.parentElement?.getBoundingClientRect().height || height;
    handle.setPointerCapture?.(e.pointerId);
    const move = (ev: PointerEvent) => resize(startHeight + startY - ev.clientY);
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      handle.removeEventListener("pointercancel", up);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", up);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const step = e.shiftKey ? 64 : 16;
    if (e.key === "ArrowUp") resize(height + step);
    else if (e.key === "ArrowDown") resize(height - step);
    else return;
    e.preventDefault();
  };

  return (
    <div
      role="separator"
      aria-orientation="horizontal"
      aria-label={t("terminal.resize")}
      aria-valuemin={MIN_HEIGHT}
      aria-valuemax={MAX_HEIGHT}
      aria-valuenow={height}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
      className="absolute inset-x-0 -top-1 z-10 h-2 cursor-row-resize outline-none hover:bg-primary/25 focus-visible:bg-primary/40"
    />
  );
}

/** "● <account> · <identity email>": who `git` in this terminal runs as. */
function AccountBadge({ repoId }: { repoId: number | null }) {
  const { t } = useTranslation();
  const { account, color, label, identity } = useRepoAccount(repoId);
  if (!account) {
    return (
      <span className="flex items-center gap-1.5 text-warning" data-testid="terminal-account">
        <span className="size-2 rounded-full bg-warning" />
        {t("terminal.noAccount")}
      </span>
    );
  }
  return (
    <span
      className="flex min-w-0 items-center gap-1.5"
      title={t("terminal.account")}
      data-testid="terminal-account"
    >
      <span className="size-2 shrink-0 rounded-full" style={{ background: color ?? undefined }} />
      <span className="font-medium text-foreground">{label}</span>
      {identity && <span className="truncate text-muted-foreground">· {identity.email}</span>}
    </span>
  );
}

/**
 * Bottom terminal panel. Every terminal of every open repo stays mounted (hidden when not shown)
 * so switching repos or hiding the panel never kills a running shell.
 */
export default function TerminalPanel({ repoId }: { repoId: number | null }) {
  const { t } = useTranslation();
  const open = useTerminal((s) => s.open);
  const height = useTerminal((s) => s.height);
  const tabs = useTerminal((s) => s.tabs);
  const activeKey = useTerminal((s) => (repoId === null ? undefined : s.active[repoId]));
  const addTab = useTerminal((s) => s.addTab);
  const closeTab = useTerminal((s) => s.closeTab);
  const activate = useTerminal((s) => s.activate);
  const setOpen = useTerminal((s) => s.setOpen);
  const repoTabs = useMemo(() => tabs.filter((tab) => tab.repoId === repoId), [tabs, repoId]);
  const shown = open && repoId !== null;

  return (
    <section
      aria-label={t("terminal.title")}
      className={shown ? "relative flex shrink-0 flex-col border-t bg-surface" : "hidden"}
      style={{ height, maxHeight: "70vh" }}
    >
      <ResizeHandle />
      <header className="flex h-8 shrink-0 items-center gap-1 border-b px-2 text-xs">
        <span title={t("terminal.copyPasteHint")} className="shrink-0 text-muted-foreground">
          <SquareTerminal className="size-3.5" aria-hidden />
        </span>
        <div role="tablist" aria-label={t("terminal.tabs")} className="flex min-w-0 items-center">
          {repoTabs.map((tab, i) => {
            const selected = tab.key === activeKey;
            const label = `${i + 1}: ${tab.title || t("terminal.title")}`;
            return (
              <div
                key={tab.key}
                className={cn(
                  "group flex h-6 items-center rounded-md pl-2 pr-0.5",
                  selected ? "bg-surface-2 text-foreground" : "text-muted-foreground",
                )}
              >
                <button
                  type="button"
                  role="tab"
                  aria-selected={selected}
                  onClick={() => activate(tab.repoId, tab.key)}
                  className="max-w-40 truncate hover:text-foreground"
                >
                  {label}
                </button>
                <button
                  type="button"
                  aria-label={`${t("terminal.closeTab")} ${label}`}
                  title={t("terminal.closeTab")}
                  onClick={() => closeTab(tab.key)}
                  className="ml-1 rounded p-0.5 opacity-60 hover:bg-surface hover:opacity-100"
                >
                  <X className="size-3" />
                </button>
              </div>
            );
          })}
        </div>
        <button
          type="button"
          aria-label={t("terminal.newTab")}
          title={t("terminal.newTab")}
          disabled={repoId === null}
          onClick={() => repoId !== null && addTab(repoId)}
          className="rounded p-1 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <Plus className="size-3.5" />
        </button>
        <div className="min-w-4 flex-1" />
        <AccountBadge repoId={repoId} />
        <button
          type="button"
          aria-label={t("terminal.hide")}
          title={`${t("terminal.hide")} (Ctrl+\`)`}
          onClick={() => setOpen(false)}
          className="ml-1 rounded p-1 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <ChevronDown className="size-3.5" />
        </button>
      </header>
      <div className="relative min-h-0 flex-1">
        {tabs.map((tab) => (
          <TerminalView
            key={tab.key}
            tabKey={tab.key}
            repoId={tab.repoId}
            visible={shown && tab.repoId === repoId && tab.key === activeKey}
          />
        ))}
        {shown && repoTabs.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground">
            <span>{t("terminal.empty")}</span>
            <button
              type="button"
              onClick={() => addTab(repoId)}
              className="flex items-center gap-1 rounded-md border px-2 py-1 text-foreground hover:bg-surface-2"
            >
              <Plus className="size-3.5" />
              {t("terminal.newTab")}
            </button>
          </div>
        )}
      </div>
    </section>
  );
}
