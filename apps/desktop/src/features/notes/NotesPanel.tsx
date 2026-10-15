// Phase 6.5: "My work" → "My notes": personal notes with tags, priority, due date, repo link.
import { useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import {
  CalendarClock,
  Download,
  Filter,
  FileText,
  FolderGit2,
  MoreHorizontal,
  Pin,
  Plus,
  Search,
  Tags,
  Upload,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { openRepository } from "@/features/repos/actions";
import { commands, errorMessage, type Note } from "@/ipc";
import { cn } from "@/lib/utils";
import { fileProblemText, pickOpenFile, pickSaveFile } from "@/lib/userFiles";
import { useWorkspace } from "@/stores/workspace";
import {
  allTags,
  dueState,
  filterNotes,
  newNoteInput,
  NOTE_SORTS,
  PRIORITIES,
  samePath,
  sortNotes,
  tagName,
  todayIso,
  toInput,
  toMarkdown,
  type NoteSort,
  type TagDef,
} from "./model";
import { deleteNotes, saveNote, setNotes, useNotes } from "./queries";
import { NoteEditor } from "./NoteEditor";
import { useNotesUi } from "./store";
import { TagManager } from "./TagManager";
import { TagPill } from "./TagPill";

interface Message {
  tone: "error" | "info";
  text: string;
  detail?: string;
}

export function NotesPanel({ onLeave }: { onLeave: () => void }) {
  const { t, i18n } = useTranslation();
  const notesQuery = useNotes();
  const data = notesQuery.data;
  const ui = useNotesUi();
  const [quickTitle, setQuickTitle] = useState("");
  const [message, setMessage] = useState<Message | null>(null);
  const [tagsOpen, setTagsOpen] = useState(false);
  const [deleting, setDeleting] = useState<Note | null>(null);
  const [today] = useState(() => todayIso());

  const tags = allTags(data?.tags ?? []);
  const tagById = new Map(tags.map((tag) => [tag.id, tag]));
  const label = (id: string) => {
    const tag = tagById.get(id);
    return tag ? tagName(t, tag) : "";
  };
  const notes = data?.notes ?? [];
  const filtered = sortNotes(
    filterNotes(
      notes,
      { query: ui.query, tags: ui.tags, priorities: ui.priorities, showDone: ui.showDone },
      label,
    ),
    ui.sort,
  );
  const selected = notes.find((n) => n.id === ui.selectedId) ?? null;
  const filterCount = ui.tags.length + ui.priorities.length + (ui.showDone ? 0 : 1);

  const fail = (text: string, e: unknown) =>
    setMessage({ tone: "error", text, detail: errorMessage(e) });

  const addQuick = async () => {
    const title = quickTitle.trim();
    if (!title) return;
    const before = new Set(notes.map((n) => n.id));
    try {
      const result = await saveNote({ ...newNoteInput(title), tags: [...ui.tags] });
      setQuickTitle("");
      const created = result.notes.find((n) => !before.has(n.id));
      if (created) ui.select(created.id);
    } catch (e) {
      fail(t("notes.saveFailedShort"), e);
    }
  };

  const toggle = async (note: Note, patch: { done?: boolean; pinned?: boolean }) => {
    try {
      await saveNote(toInput(note, patch));
    } catch (e) {
      fail(t("notes.saveFailedShort"), e);
    }
  };

  const remove = async () => {
    if (!deleting) return;
    const note = deleting;
    setDeleting(null);
    try {
      await deleteNotes([note.id]);
      if (ui.selectedId === note.id) ui.select(null);
      setMessage({
        tone: "info",
        text: t("notes.deleted", { title: note.title || t("notes.untitled") }),
      });
    } catch (e) {
      fail(t("notes.deleteFailed"), e);
    }
  };

  const openRepo = (path: string) => {
    const tab = useWorkspace.getState().tabs.find((x) => samePath(x.path, path));
    if (tab) useWorkspace.getState().setActive(tab.id);
    else void openRepository(path);
    onLeave();
  };

  const exportJson = async (onlyShown: boolean) => {
    try {
      const path = await pickSaveFile({
        title: t("notes.exportTitle"),
        defaultName: `nanomgg-notes-${today}.json`,
        filterName: t("notes.fileFilter"),
        extension: "json",
      });
      if (!path) return;
      const count = await commands.notesExportFile(
        path,
        onlyShown ? filtered.map((n) => n.id) : null,
      );
      setMessage({ tone: "info", text: t("notes.exported", { count, path }) });
    } catch (e) {
      fail(t("notes.exportFailed"), e);
    }
  };

  const exportMarkdown = async () => {
    try {
      const path = await pickSaveFile({
        title: t("notes.exportMarkdownTitle"),
        defaultName: `nanomgg-notes-${today}.md`,
        filterName: "Markdown",
        extension: "md",
      });
      if (!path) return;
      await commands.notesExportMarkdown(path, toMarkdown(filtered, t, label, i18n.language));
      setMessage({ tone: "info", text: t("notes.exported", { count: filtered.length, path }) });
    } catch (e) {
      fail(t("notes.exportFailed"), e);
    }
  };

  const importJson = async () => {
    try {
      const path = await pickOpenFile({
        title: t("notes.importTitle"),
        filterName: t("notes.fileFilter"),
        extension: "json",
      });
      if (!path) return;
      const report = await commands.notesImportFile(path);
      setNotes(report.data);
      if (report.problem) {
        setMessage({
          tone: "error",
          text: t("notes.importFailed"),
          detail: fileProblemText(t, report.problem),
        });
        return;
      }
      setMessage({
        tone: "info",
        text:
          t("notes.importDone", {
            added: report.added,
            updated: report.updated,
            skipped: report.skipped,
          }) + (report.tagsAdded ? " " + t("notes.importTags", { count: report.tagsAdded }) : ""),
      });
    } catch (e) {
      fail(t("notes.importFailed"), e);
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 text-xs">
      <div className="flex flex-wrap items-center gap-1.5">
        <div className="relative min-w-48 flex-[2]">
          <Plus className="pointer-events-none absolute top-2 left-2 size-3.5 text-muted-foreground" />
          <input
            value={quickTitle}
            maxLength={200}
            placeholder={t("notes.quickAdd")}
            aria-label={t("notes.quickAddLabel")}
            onChange={(e) => setQuickTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                e.preventDefault();
                void addQuick();
              }
            }}
            className="h-8 w-full rounded-md border bg-background pr-2 pl-7 text-xs focus:border-primary focus:outline-none"
          />
        </div>
        <div className="relative min-w-40 flex-1">
          <Search className="pointer-events-none absolute top-2 left-2 size-3.5 text-muted-foreground" />
          <input
            type="search"
            value={ui.query}
            placeholder={t("notes.search")}
            aria-label={t("notes.search")}
            onChange={(e) => ui.setQuery(e.target.value)}
            className="h-8 w-full rounded-md border bg-background pr-2 pl-7 text-xs focus:border-primary focus:outline-none"
          />
        </div>
        <FilterMenu tags={tags} />
        <Select value={ui.sort} onValueChange={(v) => ui.setSort(v as NoteSort)}>
          <SelectTrigger className="h-8 w-40 text-xs" aria-label={t("notes.sortLabel")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {NOTE_SORTS.map((s) => (
              <SelectItem key={`s-${s}`} value={s} className="text-xs">
                {t(`notes.sort.${s}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size="icon-sm" variant="outline" aria-label={t("notes.more")}>
              <MoreHorizontal />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-64 text-xs">
            <DropdownMenuItem className="text-xs" onSelect={() => setTagsOpen(true)}>
              <Tags />
              {t("notes.manageTags")}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              className="text-xs"
              disabled={!isTauri() || notes.length === 0}
              onSelect={() => void exportJson(false)}
            >
              <Download />
              {t("notes.exportAll", { count: notes.length })}
            </DropdownMenuItem>
            <DropdownMenuItem
              className="text-xs"
              disabled={!isTauri() || filtered.length === 0 || filtered.length === notes.length}
              onSelect={() => void exportJson(true)}
            >
              <Download />
              {t("notes.exportShown", { count: filtered.length })}
            </DropdownMenuItem>
            <DropdownMenuItem
              className="text-xs"
              disabled={!isTauri() || filtered.length === 0}
              onSelect={() => void exportMarkdown()}
            >
              <FileText />
              {t("notes.exportMarkdown", { count: filtered.length })}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              className="text-xs"
              disabled={!isTauri()}
              onSelect={() => void importJson()}
            >
              <Upload />
              {t("notes.import")}
            </DropdownMenuItem>
            <DropdownMenuLabel className="text-[11px] font-normal text-muted-foreground">
              {t("notes.exportHint")}
            </DropdownMenuLabel>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {message && (
        <div
          role={message.tone === "error" ? "alert" : "status"}
          className={cn(
            "flex items-start gap-2 rounded-md border p-2",
            message.tone === "error"
              ? "border-destructive/40 bg-destructive/5 text-destructive"
              : "bg-muted/40",
          )}
        >
          <span className="min-w-0 flex-1 break-words">
            {message.text}
            {message.detail && (
              <span className="block text-muted-foreground">{message.detail}</span>
            )}
          </span>
          <button
            type="button"
            className="text-muted-foreground hover:text-foreground"
            aria-label={t("notes.dismiss")}
            onClick={() => setMessage(null)}
          >
            ×
          </button>
        </div>
      )}

      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)] gap-3">
        <ScrollArea className="min-h-0 rounded-md border">
          {notesQuery.isError ? (
            <p className="p-3 text-destructive">
              {t("notes.loadFailed")}: {errorMessage(notesQuery.error)}
            </p>
          ) : notes.length === 0 ? (
            <div className="space-y-1 p-4 text-muted-foreground">
              <p className="font-medium text-foreground">{t("notes.emptyTitle")}</p>
              <p>{t("notes.emptyHint")}</p>
            </div>
          ) : filtered.length === 0 ? (
            <div className="space-y-2 p-4 text-muted-foreground">
              <p>{t("notes.noMatch")}</p>
              {(ui.query || filterCount > 0) && (
                <Button
                  size="xs"
                  variant="outline"
                  onClick={() => {
                    ui.clearFilters();
                    ui.setShowDone(true);
                  }}
                >
                  {t("notes.clearFilters")}
                </Button>
              )}
            </div>
          ) : (
            <ul className="divide-y">
              {filtered.map((note) => (
                <NoteRow
                  key={`n-${note.id}`}
                  note={note}
                  selected={note.id === ui.selectedId}
                  tagById={tagById}
                  label={label}
                  today={today}
                  onSelect={() => ui.select(note.id)}
                  onToggleDone={() => void toggle(note, { done: !note.done })}
                  onOpenRepo={openRepo}
                />
              ))}
            </ul>
          )}
        </ScrollArea>

        <div className="flex min-h-0 flex-col rounded-md border p-3">
          {selected ? (
            <NoteEditor
              key={`edit-${selected.id}`}
              note={selected}
              tags={tags}
              onDelete={() => setDeleting(selected)}
              onOpenRepo={openRepo}
            />
          ) : (
            <div className="m-auto max-w-xs space-y-1 text-center text-muted-foreground">
              <p>{t("notes.pickNote")}</p>
              <p className="text-[11px]">{t("notes.privacy")}</p>
            </div>
          )}
        </div>
      </div>

      <TagManager open={tagsOpen} onClose={() => setTagsOpen(false)} />
      <ConfirmDialog
        open={deleting !== null}
        title={t("notes.deleteTitle")}
        description={t("notes.deleteBody", {
          title: deleting?.title || t("notes.untitled"),
        })}
        confirmLabel={t("notes.delete")}
        destructive
        onConfirm={() => void remove()}
        onCancel={() => setDeleting(null)}
      />
    </div>
  );
}

function FilterMenu({ tags }: { tags: readonly TagDef[] }) {
  const { t } = useTranslation();
  const ui = useNotesUi();
  const count = ui.tags.length + ui.priorities.length + (ui.showDone ? 0 : 1);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="sm" variant={count ? "default" : "outline"} className="h-8 text-xs">
          <Filter />
          {count ? t("notes.filterCount", { count }) : t("notes.filter")}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-96 w-56 overflow-y-auto text-xs">
        <DropdownMenuCheckboxItem
          className="text-xs"
          checked={ui.showDone}
          onCheckedChange={(on) => ui.setShowDone(on)}
          onSelect={(e) => e.preventDefault()}
        >
          {t("notes.showDone")}
        </DropdownMenuCheckboxItem>
        <DropdownMenuSeparator />
        <DropdownMenuLabel className="text-[11px]">{t("notes.priority")}</DropdownMenuLabel>
        {PRIORITIES.map((p) => (
          <DropdownMenuCheckboxItem
            key={`fp-${p}`}
            className="text-xs"
            checked={ui.priorities.includes(p)}
            onCheckedChange={() => ui.togglePriority(p)}
            onSelect={(e) => e.preventDefault()}
          >
            {t(`notes.priorityLevel.${p}`)}
          </DropdownMenuCheckboxItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuLabel className="text-[11px]">{t("notes.tags")}</DropdownMenuLabel>
        {tags.map((tag) => (
          <DropdownMenuCheckboxItem
            key={`ft-${tag.id}`}
            className="text-xs"
            checked={ui.tags.includes(tag.id)}
            onCheckedChange={() => ui.toggleTag(tag.id)}
            onSelect={(e) => e.preventDefault()}
          >
            <TagPill color={tag.color}>{tagName(t, tag)}</TagPill>
          </DropdownMenuCheckboxItem>
        ))}
        {count > 0 && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              className="text-xs"
              onSelect={() => {
                ui.clearFilters();
                ui.setShowDone(true);
              }}
            >
              {t("notes.clearFilters")}
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

const PRIORITY_STYLE: Record<Note["priority"], string> = {
  low: "border-border text-muted-foreground",
  normal: "",
  high: "border-warning/60 text-warning",
  urgent: "border-danger font-semibold text-danger",
};

function NoteRow({
  note,
  selected,
  tagById,
  label,
  today,
  onSelect,
  onToggleDone,
  onOpenRepo,
}: {
  note: Note;
  selected: boolean;
  tagById: ReadonlyMap<string, TagDef>;
  label: (id: string) => string;
  today: string;
  onSelect: () => void;
  onToggleDone: () => void;
  onOpenRepo: (path: string) => void;
}) {
  const { t } = useTranslation();
  const due = dueState(note, today);
  const firstLine = note.body.split("\n").find((l) => l.trim()) ?? "";
  return (
    <li
      className={cn(
        "flex items-start gap-2 px-2 py-1.5 hover:bg-surface-2",
        selected && "bg-surface-2",
      )}
    >
      <input
        type="checkbox"
        checked={note.done}
        onChange={onToggleDone}
        aria-label={note.done ? t("notes.markOpen") : t("notes.markDone")}
        className="mt-0.5 accent-primary"
      />
      <button
        type="button"
        onClick={onSelect}
        aria-current={selected || undefined}
        className="flex min-w-0 flex-1 flex-col gap-0.5 text-left"
      >
        <span className="flex min-w-0 items-center gap-1.5">
          {note.pinned && (
            <Pin className="size-3 shrink-0 text-primary" aria-label={t("notes.pinned")} />
          )}
          <span
            className={cn(
              "truncate font-medium",
              note.done && "text-muted-foreground line-through",
            )}
          >
            {note.title || t("notes.untitled")}
          </span>
          {note.priority !== "normal" && (
            <span
              className={cn(
                "shrink-0 rounded border px-1 text-[10px] leading-4",
                PRIORITY_STYLE[note.priority],
              )}
            >
              {t(`notes.priorityLevel.${note.priority}`)}
            </span>
          )}
        </span>
        {firstLine && note.title && (
          <span className="truncate text-[11px] text-muted-foreground">{firstLine}</span>
        )}
        {(note.tags.length > 0 || note.due || note.repoName) && (
          <span className="flex min-w-0 flex-wrap items-center gap-1">
            {note.tags.map((id) => {
              const tag = tagById.get(id);
              return tag ? (
                <TagPill key={`t-${id}`} color={tag.color}>
                  {label(id)}
                </TagPill>
              ) : null;
            })}
            {note.due && (
              <span
                className={cn(
                  "inline-flex items-center gap-0.5 text-[10px]",
                  due === "overdue"
                    ? "font-medium text-danger"
                    : due === "today"
                      ? "font-medium text-warning"
                      : "text-muted-foreground",
                )}
              >
                <CalendarClock className="size-3" />
                {due === "overdue"
                  ? t("notes.overdue", { date: note.due })
                  : due === "today"
                    ? t("notes.dueToday")
                    : note.due}
              </span>
            )}
          </span>
        )}
      </button>
      {note.repoName && note.repoPath && (
        <button
          type="button"
          title={t("notes.goToRepoTitle", { path: note.repoPath })}
          onClick={() => onOpenRepo(note.repoPath!)}
          className="inline-flex max-w-32 shrink-0 items-center gap-0.5 truncate rounded px-1 text-[10px] text-muted-foreground hover:bg-surface hover:text-primary"
        >
          <FolderGit2 className="size-3 shrink-0" />
          <span className="truncate">{note.repoName}</span>
        </button>
      )}
    </li>
  );
}
