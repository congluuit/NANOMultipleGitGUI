import { useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  Copy,
  File,
  FilePen,
  FilePlus,
  Folder,
  FolderOpen,
  FolderPlus,
  Pencil,
  RefreshCw,
  Trash,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { normalize } from "@/features/command-palette/fuzzy";
import { openInEditor, useRepoFiles } from "@/features/editor/open";
import { bufferKey, useDirtyBuffers } from "@/features/editor/store";
import { FileViewMenuItems } from "@/features/file-history/FileMenuItems";
import { KIND_CLASS, kindLetter } from "@/features/worktree/format";
import { useWorktreeStatus } from "@/features/worktree/queries";
import type { ChangeKind } from "@/ipc";
import { cn } from "@/lib/utils";
import { useOpenDiff } from "@/stores/worktree";
import { openFileDialog, useExpandedFolders } from "./store";
import { baseName, buildTree, parentOf, type Tree } from "./tree";

/** Rows rendered at most (a huge expanded folder would freeze the sidebar). */
const MAX_ROWS = 3000;
const NO_FOLDERS: string[] = [];

interface Row {
  path: string;
  depth: number;
  folder: boolean;
}

function visibleRows(tree: Tree, expanded: ReadonlySet<string>): Row[] {
  const rows: Row[] = [];
  const walk = (folder: string, depth: number) => {
    const node = tree.get(folder);
    if (!node) return;
    for (const f of node.folders) {
      if (rows.length >= MAX_ROWS) return;
      rows.push({ path: f, depth, folder: true });
      if (expanded.has(f)) walk(f, depth + 1);
    }
    for (const f of node.files) {
      if (rows.length >= MAX_ROWS) return;
      rows.push({ path: f, depth, folder: false });
    }
  };
  walk("", 0);
  return rows;
}

/** The repository's files as a tree (sidebar "Files" view): open, create, rename, delete. */
export function FileTree({ repoId, filter }: { repoId: number; filter: string }) {
  const { t } = useTranslation();
  const files = useRepoFiles(repoId);
  const status = useWorktreeStatus(repoId);
  const expandedList = useExpandedFolders((s) => s.expanded[repoId] ?? NO_FOLDERS);
  const toggle = useExpandedFolders((s) => s.toggle);
  const collapseAll = useExpandedFolders((s) => s.collapseAll);
  const dirty = useDirtyBuffers((s) => s.dirty);
  const open = useOpenDiff(repoId);
  const [target, setTarget] = useState<Row | null>(null);

  const data = files.data;
  const tree = useMemo(
    () => (data ? buildTree([...data.tracked, ...data.untracked], data.folders) : null),
    [data],
  );
  const untracked = useMemo(() => new Set(data?.untracked), [data]);
  const kinds = useMemo(() => {
    const map = new Map<string, ChangeKind>();
    for (const e of [...(status.data?.staged ?? []), ...(status.data?.unstaged ?? [])]) {
      map.set(e.path, e.kind);
    }
    return map;
  }, [status.data]);

  const q = normalize(filter.trim());
  const rows = useMemo<Row[]>(() => {
    if (!tree || !data) return [];
    if (!q) return visibleRows(tree, new Set(expandedList));
    return [...data.tracked, ...data.untracked]
      .filter((p) => normalize(p).includes(q))
      .slice(0, MAX_ROWS)
      .map((path) => ({ path, depth: 0, folder: false }));
  }, [tree, data, q, expandedList]);

  const parentFor = (row: Row | null) => (row ? (row.folder ? row.path : parentOf(row.path)) : "");

  return (
    <div className="flex min-h-0 flex-col">
      <div className="flex h-7 items-center gap-0.5 px-2 text-[11px] text-muted-foreground">
        <span className="flex-1 truncate">
          {data ? t("files.count", { count: data.tracked.length + data.untracked.length }) : ""}
        </span>
        <TreeButton
          label={t("files.newFile")}
          onClick={() => openFileDialog({ kind: "newFile", repoId, parent: "" })}
        >
          <FilePlus />
        </TreeButton>
        <TreeButton
          label={t("files.newFolder")}
          onClick={() => openFileDialog({ kind: "newFolder", repoId, parent: "" })}
        >
          <FolderPlus />
        </TreeButton>
        <TreeButton label={t("files.collapseAll")} onClick={() => collapseAll(repoId)}>
          <ChevronsDownUp />
        </TreeButton>
        <TreeButton label={t("files.refresh")} onClick={() => void files.refetch()}>
          <RefreshCw className={cn(files.isFetching && "animate-spin")} />
        </TreeButton>
      </div>
      {files.isError && <p className="px-3 py-2 text-xs text-danger">{t("files.loadError")}</p>}
      {data?.truncated && (
        <p className="px-3 pb-1 text-[11px] text-warning">{t("files.truncated")}</p>
      )}
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <ul
            className="min-h-16 pb-2"
            aria-label={t("files.title")}
            onContextMenu={(e) => {
              const el = (e.target as HTMLElement).closest<HTMLElement>("[data-row]");
              const index = el ? Number(el.dataset.row) : -1;
              setTarget(rows[index] ?? null);
            }}
          >
            {rows.map((row, index) => {
              const name = q ? row.path : baseName(row.path);
              const kind = row.folder ? undefined : kinds.get(row.path);
              const isDirty = !row.folder && !!dirty[bufferKey(repoId, row.path)];
              const expanded = row.folder && expandedList.includes(row.path);
              const Icon = row.folder ? (expanded ? FolderOpen : Folder) : File;
              const Chevron = expanded ? ChevronDown : ChevronRight;
              return (
                <li key={row.path} data-row={index}>
                  <button
                    type="button"
                    title={row.path}
                    onClick={() =>
                      row.folder
                        ? toggle(repoId, row.path)
                        : openInEditor(repoId, row.path, { untracked: untracked.has(row.path) })
                    }
                    style={{ paddingLeft: 8 + row.depth * 12 }}
                    className={cn(
                      "flex h-6 w-full items-center gap-1 pr-2 text-left text-xs hover:bg-surface-2",
                      !row.folder && open?.path === row.path && "bg-surface-2 font-semibold",
                    )}
                  >
                    {row.folder ? (
                      <Chevron className="size-3.5 shrink-0 text-muted-foreground" />
                    ) : (
                      <span className="w-3.5 shrink-0" />
                    )}
                    <Icon className="size-3.5 shrink-0 text-muted-foreground" />
                    <span
                      className={cn(
                        "min-w-0 flex-1 truncate",
                        kind && KIND_CLASS[kind],
                        untracked.has(row.path) && !kind && "text-success",
                      )}
                    >
                      {name}
                    </span>
                    {isDirty && (
                      <span className="shrink-0 text-[10px] text-warning">
                        {t("editor.unsaved")}
                      </span>
                    )}
                    {kind && (
                      <span className={cn("w-3 shrink-0 font-mono text-[10px]", KIND_CLASS[kind])}>
                        {kindLetter(kind)}
                      </span>
                    )}
                  </button>
                </li>
              );
            })}
            {rows.length >= MAX_ROWS && (
              <li className="px-3 py-1 text-[11px] text-muted-foreground">{t("files.tooMany")}</li>
            )}
            {data && rows.length === 0 && (
              <li className="px-3 py-1 text-[11px] text-muted-foreground italic">
                {q ? t("files.noMatch") : t("files.empty")}
              </li>
            )}
          </ul>
        </ContextMenuTrigger>
        <ContextMenuContent>
          {target && !target.folder && (
            <ContextMenuItem
              onSelect={() =>
                openInEditor(repoId, target.path, { untracked: untracked.has(target.path) })
              }
            >
              <FilePen />
              {t("files.open")}
            </ContextMenuItem>
          )}
          <ContextMenuItem
            onSelect={() => openFileDialog({ kind: "newFile", repoId, parent: parentFor(target) })}
          >
            <FilePlus />
            {t("files.newFile")}
          </ContextMenuItem>
          <ContextMenuItem
            onSelect={() =>
              openFileDialog({ kind: "newFolder", repoId, parent: parentFor(target) })
            }
          >
            <FolderPlus />
            {t("files.newFolder")}
          </ContextMenuItem>
          {target && (
            <>
              <ContextMenuSeparator />
              <ContextMenuItem
                onSelect={() =>
                  openFileDialog({
                    kind: "rename",
                    repoId,
                    path: target.path,
                    folder: target.folder,
                  })
                }
              >
                <Pencil />
                {t("files.renameMenu")}
              </ContextMenuItem>
              <ContextMenuItem
                variant="destructive"
                onSelect={() =>
                  openFileDialog({
                    kind: "delete",
                    repoId,
                    path: target.path,
                    folder: target.folder,
                    untracked: untracked.has(target.path),
                  })
                }
              >
                <Trash />
                {t("files.deleteMenu")}
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem onSelect={() => void navigator.clipboard.writeText(target.path)}>
                <Copy />
                {t("wip.copyPath")}
              </ContextMenuItem>
              {!target.folder && !untracked.has(target.path) && (
                <FileViewMenuItems repoId={repoId} path={target.path} />
              )}
            </>
          )}
        </ContextMenuContent>
      </ContextMenu>
    </div>
  );
}

function TreeButton({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={onClick}
      className="flex size-6 items-center justify-center rounded text-muted-foreground hover:bg-surface-2 hover:text-foreground [&_svg]:size-3.5"
    >
      {children}
    </button>
  );
}
