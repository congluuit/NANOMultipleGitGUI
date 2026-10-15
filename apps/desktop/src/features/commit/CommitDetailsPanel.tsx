import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { Check, Copy, LoaderCircle, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ScrollArea } from "@/components/ui/scroll-area";
import { CommitFilesMenu } from "@/features/file-history/CommitFileMenu";
import { formatDate } from "@/features/graph/draw";
import { commands, errorMessage, type ChangedFile, type Person } from "@/ipc";
import { cn } from "@/lib/utils";
import { useGraphStore } from "@/stores/graph";
import { useOpenDiff, useWorktreeStore } from "@/stores/worktree";
import { SignatureRow } from "@/features/signing/SignatureRow";
import { fileStatusLetter, splitPath } from "./format";

const MAX_FILES = 2000;

const STATUS_CLASS: Record<string, string> = {
  A: "text-success",
  M: "text-warning",
  D: "text-danger",
  R: "text-info",
  C: "text-info",
  T: "text-warning",
  "?": "text-muted-foreground",
};

function CopyHash({ hash }: { hash: string }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      title={t("details.copy")}
      aria-label={t("details.copy")}
      onClick={() => {
        void navigator.clipboard.writeText(hash).then(() => {
          setCopied(true);
          setTimeout(() => setCopied(false), 1200);
        });
      }}
      className="inline-flex items-center gap-1 rounded px-1 font-mono text-[11px] hover:bg-surface-2"
    >
      {hash.slice(0, 10)}
      {copied ? <Check className="size-3 text-success" /> : <Copy className="size-3" />}
    </button>
  );
}

function PersonLine({ label, person, locale }: { label: string; person: Person; locale: string }) {
  return (
    <div className="grid grid-cols-[84px_1fr] gap-2">
      <span className="text-muted-foreground">{label}</span>
      <span className="min-w-0">
        <span className="block truncate" title={person.email}>
          {person.name} <span className="text-muted-foreground">&lt;{person.email}&gt;</span>
        </span>
        <span className="block text-[11px] text-muted-foreground">
          {formatDate(person.time, locale)}
        </span>
      </span>
    </div>
  );
}

function FileRow({
  file,
  active,
  onOpen,
}: {
  file: ChangedFile;
  active: boolean;
  onOpen: () => void;
}) {
  const { t } = useTranslation();
  const letter = fileStatusLetter(file.status);
  const { dir, name } = splitPath(file.path);
  return (
    <li
      role="button"
      tabIndex={0}
      data-path={file.path}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen();
        }
      }}
      className={cn(
        "flex cursor-default items-center gap-2 px-3 py-1 text-xs hover:bg-surface-2",
        active && "bg-primary/10 shadow-[inset_2px_0_0_var(--primary)]",
      )}
      title={file.oldPath ? `${file.oldPath} → ${file.path}` : file.path}
    >
      <span className={cn("w-3 shrink-0 font-mono font-bold", STATUS_CLASS[letter])}>{letter}</span>
      <span className="min-w-0 flex-1">
        <span className="block truncate">
          <span className="text-muted-foreground">{dir}</span>
          {name}
        </span>
        {file.oldPath && (
          <span className="block truncate text-[11px] text-muted-foreground">
            {t("details.renamedFrom", { path: file.oldPath })}
          </span>
        )}
      </span>
      {file.additions === null ? (
        <span className="shrink-0 text-[11px] text-muted-foreground">{t("details.binary")}</span>
      ) : (
        <span className="shrink-0 font-mono text-[11px] tabular-nums">
          <span className="text-success">+{file.additions}</span>{" "}
          <span className="text-danger">−{file.deletions}</span>
        </span>
      )}
    </li>
  );
}

export function CommitDetailsPanel({ repoId, hash }: { repoId: number; hash: string }) {
  const { t, i18n } = useTranslation();
  const select = useGraphStore((s) => s.select);
  const requestJump = useGraphStore((s) => s.requestJump);
  const openDiff = useWorktreeStore((s) => s.openDiff);
  const diff = useOpenDiff(repoId);
  const details = useQuery({
    queryKey: ["commit", repoId, hash],
    queryFn: () => commands.commitDetails(repoId, hash),
    enabled: isTauri(),
    staleTime: Infinity,
  });
  const d = details.data;
  const sameCommitter =
    d && d.author.email === d.committer.email && d.author.name === d.committer.name;

  return (
    <aside className="flex min-w-0 flex-1 flex-col bg-surface" aria-label={t("details.title")}>
      <div className="flex h-8 shrink-0 items-center justify-between border-b px-3 text-xs font-semibold">
        {t("details.title")}
        <button
          type="button"
          aria-label={t("details.close")}
          title={t("details.close")}
          onClick={() => select(repoId, null)}
          className="rounded p-0.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <X className="size-3.5" />
        </button>
      </div>
      {details.isPending && (
        <div className="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground">
          <LoaderCircle className="size-4 animate-spin" />
          {t("details.loading")}
        </div>
      )}
      {details.isError && (
        <div className="p-4 text-xs text-danger">
          {t("details.error", { message: errorMessage(details.error) })}
        </div>
      )}
      {d && (
        <ScrollArea className="min-h-0 flex-1">
          <div className="space-y-3 border-b p-3 text-xs">
            <h2 className="text-sm leading-snug font-semibold break-words">{d.summary}</h2>
            {d.body && (
              <p className="max-h-48 overflow-y-auto font-mono text-[11px] leading-relaxed break-words whitespace-pre-wrap text-muted-foreground">
                {d.body}
              </p>
            )}
            <div className="space-y-2">
              <div className="grid grid-cols-[84px_1fr] items-center gap-2">
                <span className="text-muted-foreground">{t("details.sha")}</span>
                <span>
                  <CopyHash hash={d.hash} />
                </span>
              </div>
              {d.parents.length > 0 && (
                <div className="grid grid-cols-[84px_1fr] items-center gap-2">
                  <span className="text-muted-foreground">{t("details.parents")}</span>
                  <span className="flex flex-wrap gap-1">
                    {d.parents.map((p) => (
                      <button
                        key={p}
                        type="button"
                        onClick={() => requestJump(repoId, p)}
                        className="rounded border px-1 font-mono text-[11px] text-primary hover:bg-surface-2"
                        title={p}
                      >
                        {p.slice(0, 7)}
                      </button>
                    ))}
                  </span>
                </div>
              )}
              <PersonLine label={t("details.author")} person={d.author} locale={i18n.language} />
              <SignatureRow repoId={repoId} hash={d.hash} kind={d.signature} />
              {!sameCommitter && (
                <PersonLine
                  label={t("details.committer")}
                  person={d.committer}
                  locale={i18n.language}
                />
              )}
            </div>
          </div>
          <div className="flex items-center justify-between px-3 pt-2 pb-1 text-[11px] font-semibold text-muted-foreground uppercase">
            <span>{t("details.files", { count: d.files.length })}</span>
            <span className="font-mono normal-case tabular-nums">
              <span className="text-success">+{d.additions}</span>{" "}
              <span className="text-danger">−{d.deletions}</span>
            </span>
          </div>
          {d.comparedTo && (
            <p className="px-3 pb-1 text-[11px] text-muted-foreground">
              {t("details.comparedTo", { oid: d.comparedTo.slice(0, 7) })}
            </p>
          )}
          {d.files.length === 0 ? (
            <p className="px-3 py-2 text-xs text-muted-foreground">{t("details.noFiles")}</p>
          ) : (
            <CommitFilesMenu repoId={repoId} commit={d.hash} files={d.files}>
              <ul className="pb-3">
                {d.files.slice(0, MAX_FILES).map((f) => (
                  <FileRow
                    key={`${f.oldPath ?? ""}>${f.path}`}
                    file={f}
                    active={
                      diff?.side === "commit" && diff.commit === d.hash && diff.path === f.path
                    }
                    onOpen={() =>
                      openDiff(repoId, {
                        side: "commit",
                        path: f.path,
                        oldPath: f.oldPath ?? undefined,
                        commit: d.hash,
                      })
                    }
                  />
                ))}
                {d.files.length > MAX_FILES && (
                  <li className="px-3 py-1 text-xs text-muted-foreground">
                    {t("details.moreFiles", { count: d.files.length - MAX_FILES })}
                  </li>
                )}
              </ul>
            </CommitFilesMenu>
          )}
        </ScrollArea>
      )}
    </aside>
  );
}
