import { lazy, Suspense, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { worktreeKeys } from "@/features/worktree/queries";
import { commands, errorMessage, type DiffRequest, type FileContents } from "@/ipc";
import { useWorktreeStore } from "@/stores/worktree";
import { ComparePicker, type CompareTarget } from "./ComparePicker";

const FileEditor = lazy(() => import("./FileEditor"));

interface Props {
  repoId: number;
  request: DiffRequest;
  /** Both sides of the diff (`fileContents`): the base for the markers, the text when read-only. */
  contents: { data?: FileContents; isError: boolean; error: unknown };
}

/**
 * The "File" mode of the diff view: the whole file with its changes marked. Working-tree files
 * (unstaged and new) are edited in place and saved to disk; the staged copy and files of a
 * commit are shown read-only. The changes can be measured against another branch, tag or
 * commit instead, whose version can then be restored block by block or as a whole.
 */
export function FileBody({ repoId, request, contents }: Props) {
  const { t } = useTranslation();
  const openDiff = useWorktreeStore((s) => s.openDiff);
  const worktree = request.side === "unstaged" || request.side === "untracked";
  const file = useQuery({
    queryKey: worktreeKeys.file(repoId, request.path),
    queryFn: () => commands.worktreeFileRead(repoId, request.path),
    enabled: isTauri() && worktree,
  });
  // The comparison chosen for this file (another file starts with its default base again).
  const [chosen, setChosen] = useState<{ path: string; target: CompareTarget } | null>(null);
  const compare = chosen?.path === request.path ? chosen.target : null;
  const other = useQuery({
    queryKey: ["fileAt", repoId, compare?.oid ?? "", request.path],
    queryFn: () => commands.fileAtCommit(repoId, compare?.oid ?? "", request.path),
    enabled: isTauri() && compare !== null,
    staleTime: Infinity,
  });

  const error = contents.isError
    ? contents.error
    : file.isError
      ? file.error
      : other.isError
        ? other.error
        : null;
  if (error) return <Centered>{t("diff.error", { message: errorMessage(error) })}</Centered>;
  const c = contents.data;
  const f = file.data;
  if (!c || (worktree && !f)) return <Loading />;
  // Until the other version is read, the default comparison stays on screen.
  const shown = compare && other.data ? compare : null;

  const defaultLabel =
    request.side === "unstaged"
      ? t("editor.base.index")
      : request.side === "staged"
        ? t("editor.base.head")
        : request.side === "commit"
          ? t("editor.base.parent")
          : t("editor.base.nothing");
  let base = request.side === "untracked" ? null : c.original;
  let compareNote: string | null = null;
  if (shown && other.data) {
    const o = other.data;
    if (!o.exists) {
      base = "";
      compareNote = t("editor.compareMissing", { name: shown.label });
    } else if (o.text === null || o.binary || o.lfs) {
      base = null;
      compareNote = t("editor.compareUnreadable", { name: shown.label });
    } else {
      base = o.text;
    }
  }
  const picker = (
    <ComparePicker
      repoId={repoId}
      defaultLabel={defaultLabel}
      value={compare}
      onChange={(target) => setChosen(target ? { path: request.path, target } : null)}
    />
  );
  const editorKey = `${request.side}:${request.path}:${request.commit ?? ""}`;
  const show = (text: string, version: string | null, note?: ReactNode) => (
    <Suspense fallback={<Loading />}>
      <FileEditor
        key={editorKey}
        repoId={repoId}
        path={request.path}
        base={base}
        baseLabel={shown?.label ?? (request.side === "untracked" ? null : defaultLabel)}
        text={text}
        version={version}
        note={
          compareNote || note ? (
            <>
              {compareNote}
              {compareNote && note ? " " : null}
              {note}
            </>
          ) : null
        }
        stageMode={
          shown
            ? null
            : request.side === "unstaged"
              ? "stage"
              : request.side === "staged"
                ? "unstage"
                : null
        }
        blameRev={
          request.side === "unstaged"
            ? null
            : request.side === "commit"
              ? (request.commit ?? undefined)
              : undefined
        }
        toolbarExtra={picker}
        canRestore={shown !== null && base !== null}
      />
    </Suspense>
  );

  if (worktree && f) {
    if (!f.exists) return <Centered>{t("editor.missing")}</Centered>;
    if (f.binary) return <Centered>{t("diff.binary")}</Centered>;
    if (f.tooLarge) return <Centered>{t("diff.tooLarge")}</Centered>;
    if (f.text !== null) return show(f.text, f.version);
    // Symlinks and other encodings: shown (lossy) but never written back.
    if (c.binary || c.tooLarge || c.modified === null)
      return <Centered>{t("diff.binary")}</Centered>;
    return show(
      c.modified,
      null,
      f.symlink ? t("editor.readOnly.symlink") : t("editor.readOnly.notUtf8"),
    );
  }

  if (c.binary) return <Centered>{t("diff.binary")}</Centered>;
  if (c.tooLarge) return <Centered>{t("diff.tooLarge")}</Centered>;
  if (c.modified === null) return <Centered>{t("diff.deletedFile")}</Centered>;
  const note =
    request.side === "staged" ? (
      <span>
        {t("editor.readOnly.staged")}{" "}
        <button
          type="button"
          className="text-primary hover:underline"
          onClick={() => openDiff(repoId, { side: "unstaged", path: request.path })}
        >
          {t("editor.editWorkingCopy")}
        </button>
      </span>
    ) : (
      t("editor.readOnly.commit")
    );
  return show(c.modified, null, note);
}

function Centered({ children }: { children: ReactNode }) {
  return (
    <div className="flex flex-1 items-center justify-center p-6 text-center text-xs text-muted-foreground">
      {children}
    </div>
  );
}

function Loading() {
  const { t } = useTranslation();
  return (
    <Centered>
      <LoaderCircle className="mr-2 size-4 animate-spin" />
      {t("diff.loading")}
    </Centered>
  );
}
