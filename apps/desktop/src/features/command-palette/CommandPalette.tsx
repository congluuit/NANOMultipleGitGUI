import { useEffect, useId, useRef, useState } from "react";
import { Check, ChevronLeft, ChevronRight, Command as CommandIcon, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { errorMessage } from "@/ipc";
import { cn } from "@/lib/utils";
import { useUi } from "@/stores/ui";
import { PaletteConfirmHost } from "./ConfirmHost";
import { usePaletteContext } from "./context";
import { recentId, subSections, topLevelSections, type PaletteItem } from "./model";
import { usePaletteCommands, type PaletteCommand } from "./registry";
import {
  isPaletteShortcut,
  pushRecent,
  readRecent,
  toggleCommandPalette,
  usePalette,
  type PaletteStart,
} from "./store";

function runSafely(fn: () => unknown) {
  try {
    void Promise.resolve(fn()).catch((e: unknown) =>
      useUi.getState().showNotice({ kind: "error", message: errorMessage(e) }),
    );
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
  }
}

/** The command palette dialog plus its global `Ctrl/Cmd+K` shortcut. Mount once. */
export function CommandPalette() {
  const { t } = useTranslation();
  const open = usePalette((s) => s.open);
  const start = usePalette((s) => s.start);
  const setOpen = usePalette((s) => s.setOpen);
  // A command ran: it decides where focus goes, so don't restore it to the trigger.
  const ran = useRef(false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!isPaletteShortcut(e)) return;
      // Keep readline's Ctrl+K (kill line) inside the integrated terminal.
      if (e.target instanceof Element && e.target.closest(".xterm")) return;
      e.preventDefault();
      e.stopPropagation();
      toggleCommandPalette();
    };
    // Capture phase: editors (Monaco) must not swallow the shortcut.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  return (
    <>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent
          showCloseButton={false}
          className="top-[12%] translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-xl"
          onOpenAutoFocus={(e) => e.preventDefault()}
          onCloseAutoFocus={(e) => {
            if (ran.current) e.preventDefault();
            ran.current = false;
          }}
        >
          <DialogTitle className="sr-only">{t("cmd.title")}</DialogTitle>
          <DialogDescription className="sr-only">{t("cmd.description")}</DialogDescription>
          <PaletteBody
            start={start}
            onRun={() => {
              ran.current = true;
              setOpen(false);
            }}
          />
        </DialogContent>
      </Dialog>
      <PaletteConfirmHost />
    </>
  );
}

function initialStack(commands: PaletteCommand[], start: PaletteStart | null): PaletteCommand[] {
  const cmd = start?.commandId ? commands.find((c) => c.id === start.commandId) : undefined;
  return cmd && (cmd.children || cmd.input) ? [cmd] : [];
}

function PaletteBody({ start, onRun }: { start: PaletteStart | null; onRun: () => void }) {
  const { t } = useTranslation();
  const ctx = usePaletteContext();
  const commands = usePaletteCommands(ctx);
  const [stack, setStack] = useState<PaletteCommand[]>(() => initialStack(commands, start));
  const [query, setQuery] = useState(start?.query ?? "");
  const [active, setActive] = useState(0);
  const [recent] = useState(readRecent);
  const [inputError, setInputError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const baseId = useId();

  // Re-resolve the step from the fresh command list so its children see the current context.
  const parent = stack.length
    ? (commands.find((c) => c.id === stack[stack.length - 1]!.id) ?? stack[stack.length - 1]!)
    : null;
  const prompt = parent?.input ?? null;
  const sections = prompt
    ? []
    : parent
      ? subSections(parent, ctx, query)
      : topLevelSections(commands, ctx, query, recent, t("cmd.group.recent"));
  const items = sections.flatMap((s) => s.items);
  const current = items.length ? Math.min(active, items.length - 1) : -1;
  const optionId = (i: number) => `${baseId}-opt-${i}`;

  useEffect(() => {
    if (current >= 0) {
      document.getElementById(`${baseId}-opt-${current}`)?.scrollIntoView({ block: "nearest" });
    }
  }, [baseId, current]);

  const reset = (next: PaletteCommand[]) => {
    setStack(next);
    setQuery("");
    setActive(0);
    setInputError(null);
    inputRef.current?.focus();
  };

  const finish = (id: string, fn: () => unknown) => {
    pushRecent(id);
    onRun();
    runSafely(fn);
  };

  const choose = (it: PaletteItem | undefined) => {
    if (!it || !it.enabled) return;
    const { cmd } = it;
    if (cmd.children || cmd.input) {
      reset([...stack, ...(it.parent ? [it.parent] : []), cmd]);
      return;
    }
    const id = it.parent ? recentId(cmd, it.parent) : recentId(cmd, parent);
    finish(id, () => cmd.run?.(ctx));
  };

  const submitPrompt = () => {
    if (!parent || !prompt) return;
    const error = prompt.validate?.(query) ?? null;
    if (error) {
      setInputError(error);
      return;
    }
    finish(parent.id, () => prompt.run(query, ctx));
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.nativeEvent.isComposing) return;
    switch (e.key) {
      case "ArrowDown":
      case "ArrowUp": {
        e.preventDefault();
        if (!items.length) return;
        const step = e.key === "ArrowDown" ? 1 : -1;
        setActive((current + step + items.length) % items.length);
        return;
      }
      case "Enter":
        e.preventDefault();
        if (prompt) submitPrompt();
        else choose(items[current]);
        return;
      case "Backspace":
        if (query === "" && stack.length) {
          e.preventDefault();
          reset(stack.slice(0, -1));
        }
        return;
    }
  };

  const placeholder = prompt ? prompt.placeholder : parent ? t("cmd.filter") : t("cmd.placeholder");
  // Index of each section's first item in `items`.
  const offsets = sections.map((_, si) =>
    sections.slice(0, si).reduce((n, s) => n + s.items.length, 0),
  );

  return (
    <div className="flex max-h-[min(32rem,70vh)] flex-col text-sm">
      <div className="flex items-center gap-2 border-b px-3">
        {parent ? (
          <button
            type="button"
            aria-label={t("cmd.back")}
            title={t("cmd.back")}
            onClick={() => reset(stack.slice(0, -1))}
            className="rounded p-0.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
          >
            <ChevronLeft className="size-4" />
          </button>
        ) : (
          <Search className="size-4 shrink-0 text-muted-foreground" />
        )}
        {stack.map((s) => (
          <span
            key={s.id}
            className="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 text-xs text-muted-foreground"
          >
            {s.title.replace(/…$/, "")}
          </span>
        ))}
        <input
          ref={inputRef}
          autoFocus
          role="combobox"
          aria-expanded={!prompt}
          aria-controls={prompt ? undefined : `${baseId}-list`}
          aria-activedescendant={current >= 0 ? optionId(current) : undefined}
          aria-autocomplete="list"
          aria-label={placeholder}
          aria-invalid={inputError ? true : undefined}
          placeholder={placeholder}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
            setInputError(null);
          }}
          onKeyDown={onKeyDown}
          spellCheck={false}
          autoComplete="off"
          className="h-11 min-w-0 flex-1 bg-transparent outline-none placeholder:text-muted-foreground"
        />
      </div>

      {prompt ? (
        <div className="px-3 py-3 text-xs" aria-live="polite">
          {inputError ? (
            <span className="text-danger">{inputError}</span>
          ) : (
            <span className="text-muted-foreground">{t("cmd.promptHint")}</span>
          )}
        </div>
      ) : (
        <div
          id={`${baseId}-list`}
          role="listbox"
          aria-label={parent?.title ?? t("cmd.title")}
          className="min-h-0 flex-1 overflow-y-auto p-1"
        >
          {items.length === 0 && (
            <div className="px-3 py-6 text-center text-xs text-muted-foreground">
              {parent && !query ? t("cmd.emptyList") : t("cmd.empty")}
            </div>
          )}
          {sections.map((section, si) => (
            <div key={section.group || "_"} role="group" aria-label={section.group || undefined}>
              {section.group && (
                <div
                  role="presentation"
                  className="px-2 pt-2 pb-1 text-[11px] font-semibold text-muted-foreground uppercase"
                >
                  {section.group}
                </div>
              )}
              {section.items.map((it, k) => {
                const i = offsets[si]! + k;
                return (
                  <Row
                    key={`${section.group}:${it.key}`}
                    id={optionId(i)}
                    item={it}
                    active={i === current}
                    onHover={() => i !== current && setActive(i)}
                    onChoose={() => choose(it)}
                  />
                );
              })}
            </div>
          ))}
        </div>
      )}

      <div className="flex items-center gap-3 border-t bg-surface-2 px-3 py-1.5 text-[11px] text-muted-foreground">
        <CommandIcon className="size-3" />
        <Hint keys={["↑", "↓"]} label={t("cmd.keys.navigate")} />
        <Hint keys={["Enter"]} label={prompt ? t("cmd.keys.submit") : t("cmd.keys.run")} />
        {parent && <Hint keys={["⌫"]} label={t("cmd.keys.back")} />}
        <Hint keys={["Esc"]} label={t("cmd.keys.close")} />
      </div>
    </div>
  );
}

function Hint({ keys, label }: { keys: string[]; label: string }) {
  return (
    <span className="flex items-center gap-1">
      {keys.map((k) => (
        <kbd key={k} className="rounded border bg-surface px-1 font-mono text-[10px]">
          {k}
        </kbd>
      ))}
      {label}
    </span>
  );
}

function Row({
  id,
  item,
  active,
  onHover,
  onChoose,
}: {
  id: string;
  item: PaletteItem;
  active: boolean;
  onHover: () => void;
  onChoose: () => void;
}) {
  const { cmd } = item;
  const Icon = cmd.icon ?? item.parent?.icon;
  const opens = !!(cmd.children || cmd.input);
  return (
    <div
      id={id}
      role="option"
      aria-selected={active}
      aria-disabled={!item.enabled || undefined}
      onMouseMove={onHover}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onChoose}
      className={cn(
        "flex h-8 cursor-default items-center gap-2 rounded px-2 text-xs",
        active && "bg-surface-2",
        !item.enabled && "opacity-50",
        cmd.destructive && item.enabled && "text-danger",
      )}
    >
      <span className="flex size-4 shrink-0 items-center justify-center text-muted-foreground">
        {cmd.checked ? (
          <Check className="size-3.5 text-primary" />
        ) : (
          Icon && <Icon className="size-3.5" />
        )}
      </span>
      <span className="truncate">{item.title}</span>
      {cmd.hint && (
        <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground">
          {cmd.hint}
        </span>
      )}
      {!cmd.hint && <span className="flex-1" />}
      {cmd.shortcut && (
        <span className="flex shrink-0 gap-0.5">
          {cmd.shortcut.split(/\+(?!$)/).map((k) => (
            <kbd
              key={k}
              className="rounded border bg-surface px-1 font-mono text-[10px] text-muted-foreground"
            >
              {k}
            </kbd>
          ))}
        </span>
      )}
      {opens && <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />}
    </div>
  );
}
