import {
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
  type SyntheticEvent,
} from "react";
import { CircleDot } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { IssueSummary } from "@/ipc";
import { cn } from "@/lib/utils";
import { useIssueContext } from "./useIssueContext";
import { applyRef, matchIssues, refTrigger, type RefTrigger } from "./model";
import { useOpenIssuesForSuggest } from "./queries";

type Field = HTMLInputElement | HTMLTextAreaElement;

export interface IssueRefSuggest<E extends Field> {
  /** Call first in the field's `onKeyDown`; returns `true` when it handled the key. */
  onKeyDown: (e: KeyboardEvent<E>) => boolean;
  /** Re-reads the caret: the field's `onSelect` (fires on typing and caret moves). */
  onCaret: (e: SyntheticEvent<E>) => void;
  onBlur: () => void;
  /** The suggestion list; render it inside a `relative` wrapper of the field. */
  popup: ReactNode;
}

/**
 * `#` suggestions for a commit message field: typing `#` lists the open issues of the
 * repository's GitHub / GitLab project (filtered by number or title); Enter / Tab inserts
 * `#123`, Shift+Enter inserts `Closes #123`. Inactive when the repository has no usable remote.
 */
export function useIssueRefSuggest<E extends Field>(
  repoId: number,
  value: string,
  setValue: (value: string) => void,
): IssueRefSuggest<E> {
  const { t } = useTranslation();
  // The field, remembered from its events (a click in the list inserts into it).
  const [field, setField] = useState<E | null>(null);
  const [trigger, setTrigger] = useState<RefTrigger | null>(null);
  const [index, setIndex] = useState(0);
  const ctx = useIssueContext(repoId);
  const ready = ctx.status === "ready";
  const open = useOpenIssuesForSuggest(repoId, ready && trigger !== null);
  const items: IssueSummary[] =
    trigger && open.data ? matchIssues(open.data.page.items, trigger.query) : [];
  const shown = trigger !== null && ready;
  const active = Math.min(index, Math.max(items.length - 1, 0));
  // Caret to restore once the inserted reference is rendered. Applied in a layout effect right
  // after that commit, not in a later frame: a key typed before the next frame would otherwise
  // land at the caret first and then be jumped over (`#3` became `3#`).
  const pendingCaret = useRef<{ el: E; value: string; caret: number } | null>(null);
  useLayoutEffect(() => {
    const p = pendingCaret.current;
    if (!p || p.el.value !== p.value) return;
    pendingCaret.current = null;
    p.el.focus();
    p.el.setSelectionRange(p.caret, p.caret);
  });

  const onCaret = (e: SyntheticEvent<E>) => {
    const el = e.currentTarget;
    if (el !== field) setField(el);
    if (!ready) return;
    const caret = el.selectionStart ?? el.value.length;
    const next = el.selectionStart === el.selectionEnd ? refTrigger(el.value, caret) : null;
    if (next?.start !== trigger?.start || next?.query !== trigger?.query) {
      setTrigger(next);
      setIndex(0);
    }
  };

  const insert = (issue: IssueSummary, closes: boolean) => {
    const el = field;
    if (!el || !trigger) return;
    const caret = el.selectionStart ?? value.length;
    const text = closes ? `Closes #${issue.number}` : `#${issue.number}`;
    const next = applyRef(value, trigger, caret, text);
    pendingCaret.current = { el, value: next.value, caret: next.caret };
    setValue(next.value);
    setTrigger(null);
  };

  const onKeyDown = (e: KeyboardEvent<E>) => {
    if (!shown || items.length === 0) {
      if (shown && e.key === "Escape") {
        setTrigger(null);
        return true;
      }
      return false;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : -1;
      setIndex((active + step + items.length) % items.length);
      return true;
    }
    if ((e.key === "Enter" && !e.ctrlKey && !e.metaKey) || e.key === "Tab") {
      e.preventDefault();
      insert(items[active]!, e.key === "Enter" && e.shiftKey);
      return true;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      setTrigger(null);
      return true;
    }
    return false;
  };

  const popup = shown ? (
    <div
      role="listbox"
      aria-label={t("issues.suggest.title")}
      className="absolute top-full right-0 left-0 z-50 mt-1 max-h-56 overflow-y-auto rounded-md border bg-popover py-1 text-xs shadow-md"
      onMouseDown={(e) => e.preventDefault()}
    >
      {open.isPending && <p className="px-2 py-1 text-muted-foreground">{t("issues.loading")}</p>}
      {open.isError && (
        <p className="px-2 py-1 text-muted-foreground">{t("issues.suggest.error")}</p>
      )}
      {open.isSuccess && items.length === 0 && (
        <p className="px-2 py-1 text-muted-foreground">{t("issues.suggest.none")}</p>
      )}
      {items.map((issue, i) => (
        <div
          key={`sg-${issue.number}`}
          role="option"
          aria-selected={i === active}
          onMouseEnter={() => setIndex(i)}
          className={cn(
            "flex cursor-default items-center gap-1.5 px-2 py-1",
            i === active && "bg-surface-2",
          )}
        >
          <button
            type="button"
            onClick={() => insert(issue, false)}
            className="flex min-w-0 flex-1 items-center gap-1.5 text-left"
          >
            <CircleDot className="size-3 shrink-0 text-success" />
            <span className="shrink-0 text-muted-foreground tabular-nums">#{issue.number}</span>
            <span className="truncate">{issue.title}</span>
          </button>
          <button
            type="button"
            onClick={() => insert(issue, true)}
            title={t("issues.suggest.closesHint")}
            className="shrink-0 rounded px-1 text-[10px] text-muted-foreground hover:bg-surface hover:text-foreground"
          >
            {t("issues.suggest.closes")}
          </button>
        </div>
      ))}
      {items.length > 0 && (
        <p className="border-t px-2 pt-1 text-[10px] text-muted-foreground">
          {t("issues.suggest.keys")}
        </p>
      )}
    </div>
  ) : null;

  return { onKeyDown, onCaret, onBlur: () => setTrigger(null), popup };
}
