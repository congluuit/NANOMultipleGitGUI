import { useEffect, useRef, useState } from "react";
import { ArrowDown, ArrowUp, GripVertical } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { TodoAction } from "@/ipc";
import { cn } from "@/lib/utils";
import {
  ACTIONS,
  effectiveMessage,
  hasEditor,
  moveRow,
  moveRowTo,
  setAction,
  setMessage,
  type TodoRow,
} from "./model";

/**
 * Editable todo list: drag the handle (pointer events, works in every webview) or press
 * Alt+↑ / Alt+↓ on a focused row to reorder; one action per row; inline message editors.
 */
export function TodoEditor({
  rows,
  onChange,
}: {
  rows: TodoRow[];
  onChange: (rows: TodoRow[]) => void;
}) {
  const { t } = useTranslation();
  const list = useRef<HTMLOListElement>(null);
  const focusOid = useRef<string | null>(null);
  const [drag, setDrag] = useState<{ from: number; over: number } | null>(null);

  // Keep keyboard focus on the row that was moved.
  useEffect(() => {
    const oid = focusOid.current;
    if (!oid) return;
    focusOid.current = null;
    list.current?.querySelector<HTMLElement>(`[data-oid="${oid}"]`)?.focus();
  }, [rows]);

  const move = (index: number, delta: number) => {
    const next = moveRow(rows, index, delta);
    if (next === rows) return;
    focusOid.current = rows[index]!.oid;
    onChange(next);
  };

  /** Index the pointer is over, from the rows' vertical midpoints. */
  const indexAt = (clientY: number) => {
    const items = list.current?.querySelectorAll<HTMLElement>("[data-oid]") ?? [];
    let index = 0;
    items.forEach((el, i) => {
      const r = el.getBoundingClientRect();
      if (clientY > r.top + r.height / 2) index = i;
    });
    return index;
  };

  return (
    <ol ref={list} aria-label={t("rebase.interactive.list")} className="space-y-1">
      {rows.map((row, i) => {
        const melded = row.action === "squash" || row.action === "fixup";
        const editor = hasEditor(rows, i);
        const editorId = `todo-msg-${row.oid}`;
        return (
          <li
            key={row.oid}
            data-oid={row.oid}
            tabIndex={0}
            aria-label={t("rebase.interactive.row", { oid: row.shortOid, subject: row.subject })}
            onKeyDown={(e) => {
              if (!e.altKey || e.target !== e.currentTarget) return;
              if (e.key === "ArrowUp" || e.key === "ArrowDown") {
                e.preventDefault();
                move(i, e.key === "ArrowUp" ? -1 : 1);
              }
            }}
            className={cn(
              "rounded-md border bg-background px-1.5 py-1 text-xs focus-visible:border-primary focus-visible:outline-none",
              melded && "ml-5",
              drag?.from === i && "opacity-50",
              drag && drag.over === i && drag.from !== i && "border-primary",
            )}
          >
            <div className="flex items-center gap-1.5">
              <span
                role="button"
                aria-label={t("rebase.interactive.dragHandle", { oid: row.shortOid })}
                className="cursor-grab touch-none text-muted-foreground active:cursor-grabbing"
                onPointerDown={(e) => {
                  e.currentTarget.setPointerCapture?.(e.pointerId);
                  setDrag({ from: i, over: i });
                }}
                onPointerMove={(e) => {
                  if (drag) setDrag({ ...drag, over: indexAt(e.clientY) });
                }}
                onPointerUp={() => {
                  if (drag && drag.over !== drag.from) {
                    focusOid.current = rows[drag.from]!.oid;
                    onChange(moveRowTo(rows, drag.from, drag.over));
                  }
                  setDrag(null);
                }}
                onPointerCancel={() => setDrag(null)}
              >
                <GripVertical className="size-3.5" />
              </span>
              <span className="flex flex-col">
                <button
                  type="button"
                  tabIndex={-1}
                  aria-label={t("rebase.interactive.moveUp", { oid: row.shortOid })}
                  disabled={i === 0}
                  onClick={() => move(i, -1)}
                  className="text-muted-foreground hover:text-foreground disabled:opacity-30"
                >
                  <ArrowUp className="size-3" />
                </button>
                <button
                  type="button"
                  tabIndex={-1}
                  aria-label={t("rebase.interactive.moveDown", { oid: row.shortOid })}
                  disabled={i === rows.length - 1}
                  onClick={() => move(i, 1)}
                  className="text-muted-foreground hover:text-foreground disabled:opacity-30"
                >
                  <ArrowDown className="size-3" />
                </button>
              </span>
              <select
                aria-label={t("rebase.interactive.actionLabel", { oid: row.shortOid })}
                value={row.action}
                onChange={(e) => onChange(setAction(rows, i, e.target.value as TodoAction))}
                className="h-6 w-24 shrink-0 rounded border bg-background px-1 text-xs"
              >
                {ACTIONS.map((a) => (
                  <option key={a} value={a} title={t(`rebase.interactive.action.${a}`)}>
                    {a}
                  </option>
                ))}
              </select>
              <span className="shrink-0 font-mono text-muted-foreground">{row.shortOid}</span>
              <span
                className={cn(
                  "min-w-0 flex-1 truncate",
                  row.action === "drop" && "text-muted-foreground line-through",
                  row.action === "fixup" && "text-muted-foreground",
                )}
                title={row.original}
              >
                {row.subject}
              </span>
              {row.isMerge && (
                <span className="shrink-0 rounded bg-danger/15 px-1 text-[10px] text-danger">
                  {t("rebase.interactive.merge")}
                </span>
              )}
              {row.pushed && (
                <span
                  className="shrink-0 rounded bg-warning/15 px-1 text-[10px] text-warning"
                  title={t("rebase.interactive.pushedBadge")}
                >
                  {t("rebase.interactive.pushed")}
                </span>
              )}
            </div>
            <p className="mt-0.5 pl-12 text-[10px] text-muted-foreground">
              {t(`rebase.interactive.action.${row.action}`)}
            </p>
            {editor && (
              <div className="mt-1 pl-12">
                <label htmlFor={editorId} className="sr-only">
                  {row.action === "squash"
                    ? t("rebase.interactive.squashLabel")
                    : row.action === "edit"
                      ? t("rebase.interactive.editMessageLabel", { oid: row.shortOid })
                      : t("rebase.interactive.messageLabel", { oid: row.shortOid })}
                </label>
                {row.action === "squash" && (
                  <p className="mb-0.5 text-[10px] font-medium">
                    {t("rebase.interactive.squashLabel")}
                  </p>
                )}
                <textarea
                  id={editorId}
                  rows={row.action === "squash" ? 4 : 2}
                  value={effectiveMessage(rows, i)}
                  onChange={(e) => onChange(setMessage(rows, i, e.target.value))}
                  onKeyDown={(e) => e.stopPropagation()}
                  className="w-full rounded-md border bg-background px-1.5 py-1 font-mono text-xs focus:border-primary focus:outline-none"
                />
              </div>
            )}
          </li>
        );
      })}
    </ol>
  );
}
