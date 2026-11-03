import { useEffect, useMemo, useRef, useState } from "react";
import { File, FilePlus, LoaderCircle, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { cleanPath, createFile } from "@/features/files/actions";
import { cn } from "@/lib/utils";
import { useActiveRepo, useWorkspace } from "@/stores/workspace";
import { openInEditor, recentFiles, useRepoFiles } from "./open";
import {
  isQuickOpenShortcut,
  parseFileQuery,
  preparePaths,
  rankPaths,
  useQuickOpen,
} from "./quickOpenModel";
import { bufferKey, useDirtyBuffers } from "./store";

const LIMIT = 60;

type Item = { kind: "file"; path: string } | { kind: "create"; path: string };

/** Quick open (Ctrl+P): any file of the active repository, by fuzzy name. Mount once. */
export function QuickOpen() {
  const { t } = useTranslation();
  const open = useQuickOpen((s) => s.open);
  const repo = useActiveRepo();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!isQuickOpenShortcut(e)) return;
      if (e.target instanceof Element && e.target.closest(".xterm")) return;
      if (useWorkspace.getState().activeId === null) return;
      e.preventDefault();
      e.stopPropagation();
      useQuickOpen.setState((s) => ({ open: !s.open }));
    };
    // Capture phase, like the command palette: Monaco must not swallow it.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  return (
    <Dialog open={open && !!repo} onOpenChange={(o) => useQuickOpen.setState({ open: o })}>
      <DialogContent
        showCloseButton={false}
        className="top-[12%] translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-xl"
      >
        <DialogTitle className="sr-only">{t("quickOpen.title")}</DialogTitle>
        <DialogDescription className="sr-only">{t("quickOpen.description")}</DialogDescription>
        {repo && <QuickOpenBody repoId={repo.id} />}
      </DialogContent>
    </Dialog>
  );
}

function QuickOpenBody({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const files = useRepoFiles(repoId);
  const dirty = useDirtyBuffers((s) => s.dirty);
  const [raw, setRaw] = useState("");
  const [active, setActive] = useState(0);
  const list = useRef<HTMLUListElement>(null);

  const data = files.data;
  const all = useMemo(() => (data ? [...data.tracked, ...data.untracked] : []), [data]);
  const prepared = useMemo(() => preparePaths(all), [all]);
  const untracked = useMemo(() => new Set(data?.untracked), [data]);
  const query = parseFileQuery(raw);

  const items = useMemo<Item[]>(() => {
    if (!query.text) {
      const known = new Set(all);
      const recent = recentFiles(repoId).filter((p) => known.has(p));
      return recent.map((path) => ({ kind: "file", path }));
    }
    const found: Item[] = rankPaths(query.text, prepared, LIMIT).map((path) => ({
      kind: "file",
      path,
    }));
    const wanted = cleanPath(query.text);
    if (wanted && !all.includes(wanted) && !query.text.endsWith("/")) {
      found.push({ kind: "create", path: wanted });
    }
    return found;
  }, [query.text, prepared, all, repoId]);

  const index = Math.min(active, Math.max(0, items.length - 1));

  useEffect(() => {
    list.current
      ?.querySelector<HTMLElement>(`[data-index="${index}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const choose = (item: Item | undefined) => {
    if (!item) return;
    useQuickOpen.setState({ open: false });
    if (item.kind === "create") {
      void createFile(repoId, item.path);
      return;
    }
    openInEditor(repoId, item.path, {
      line: query.line ?? undefined,
      column: query.column ?? undefined,
      untracked: untracked.has(item.path),
    });
  };

  return (
    <div className="flex max-h-[60vh] flex-col">
      <div className="flex items-center gap-2 border-b px-3">
        <Search className="size-4 shrink-0 text-muted-foreground" />
        <input
          autoFocus
          value={raw}
          onChange={(e) => {
            setRaw(e.target.value);
            setActive(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setActive(Math.min(index + 1, items.length - 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setActive(Math.max(index - 1, 0));
            } else if (e.key === "Enter") {
              e.preventDefault();
              choose(items[index]);
            }
          }}
          placeholder={t("quickOpen.placeholder")}
          aria-label={t("quickOpen.placeholder")}
          className="h-10 min-w-0 flex-1 bg-transparent text-sm placeholder:text-muted-foreground focus:outline-none"
        />
        {files.isFetching && <LoaderCircle className="size-4 animate-spin text-muted-foreground" />}
      </div>
      <ul ref={list} role="listbox" className="min-h-0 flex-1 overflow-y-auto py-1">
        {!query.text && items.length > 0 && (
          <li className="px-3 py-1 text-[11px] font-semibold text-muted-foreground uppercase">
            {t("quickOpen.recent")}
          </li>
        )}
        {items.map((item, i) => {
          const slash = item.path.lastIndexOf("/");
          const name = item.path.slice(slash + 1);
          const dir = slash > 0 ? item.path.slice(0, slash) : "";
          const Icon = item.kind === "create" ? FilePlus : File;
          return (
            <li
              key={`${item.kind}:${item.path}`}
              data-index={i}
              role="option"
              aria-selected={i === index}
            >
              <button
                type="button"
                onMouseMove={() => setActive(i)}
                onClick={() => choose(item)}
                className={cn(
                  "flex h-8 w-full items-center gap-2 px-3 text-left text-xs",
                  i === index && "bg-surface-2",
                )}
              >
                <Icon className="size-4 shrink-0 text-muted-foreground" />
                {item.kind === "create" ? (
                  <span className="truncate">
                    {t("quickOpen.create")}{" "}
                    <span className="font-mono font-medium">{item.path}</span>
                  </span>
                ) : (
                  <>
                    <span className="shrink-0 font-medium">{name}</span>
                    <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground">
                      {dir}
                    </span>
                    {dirty[bufferKey(repoId, item.path)] && (
                      <span className="shrink-0 text-[10px] text-warning">
                        {t("editor.unsaved")}
                      </span>
                    )}
                    {untracked.has(item.path) && (
                      <span className="shrink-0 text-[10px] text-success">
                        {t("quickOpen.untracked")}
                      </span>
                    )}
                  </>
                )}
              </button>
            </li>
          );
        })}
        {query.text && items.length === 0 && data && (
          <li className="px-3 py-2 text-xs text-muted-foreground">{t("quickOpen.noMatch")}</li>
        )}
        {!query.text && items.length === 0 && (
          <li className="px-3 py-2 text-xs text-muted-foreground">{t("quickOpen.hint")}</li>
        )}
      </ul>
    </div>
  );
}
