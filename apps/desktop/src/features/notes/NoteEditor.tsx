// Phase 6.5: edit one note (saved automatically a moment after the last change).
import { useEffect, useRef, useState } from "react";
import { CheckSquare, FolderGit2, Pin, Trash2, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { errorMessage, type Note, type NoteInput, type NotesData } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { cn } from "@/lib/utils";
import { useWorkspace } from "@/stores/workspace";
import { PRIORITIES, samePath, tagName, toInput, type TagDef } from "./model";
import { notesKey, saveNote } from "./queries";
import { TagPill } from "./TagPill";

type Draft = Pick<
  NoteInput,
  "title" | "body" | "tags" | "priority" | "due" | "repoPath" | "repoName"
>;

const AUTOSAVE_MS = 700;
const NO_REPO = "__none__";

function draftOf(note: Note): Draft {
  return {
    title: note.title,
    body: note.body,
    tags: note.tags,
    priority: note.priority,
    due: note.due,
    repoPath: note.repoPath,
    repoName: note.repoName,
  };
}

/** The note as stored right now (done / pinned may have changed from the list). */
function storedNote(id: string): Note | undefined {
  return queryClient.getQueryData<NotesData>(notesKey)?.notes.find((n) => n.id === id);
}

export function NoteEditor({
  note,
  tags,
  onDelete,
  onOpenRepo,
}: {
  note: Note;
  tags: readonly TagDef[];
  onDelete: () => void;
  onOpenRepo: (path: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const openTabs = useWorkspace((s) => s.tabs);
  const [draft, setDraft] = useState<Draft>(() => draftOf(note));
  const [dirty, setDirty] = useState(false);
  const [status, setStatus] = useState<"idle" | "saving" | "saved">("idle");
  const [error, setError] = useState<string | null>(null);
  const latest = useRef<{ draft: Draft; dirty: boolean }>({ draft, dirty });

  useEffect(() => {
    latest.current = { draft, dirty };
    if (!dirty) return;
    const timer = setTimeout(() => {
      const current = storedNote(note.id);
      if (!current) return;
      if (!draft.title.trim() && !draft.body.trim()) {
        setError(t("notes.emptyNote"));
        return;
      }
      setStatus("saving");
      saveNote(toInput(current, draft))
        .then(() => {
          // Typing went on while saving: that newer text is saved by the next round.
          if (latest.current.draft !== draft) return;
          latest.current.dirty = false;
          setDirty(false);
          setError(null);
          setStatus("saved");
        })
        .catch((e: unknown) => {
          setStatus("idle");
          setError(t("notes.saveFailed", { error: errorMessage(e) }));
        });
    }, AUTOSAVE_MS);
    return () => clearTimeout(timer);
  }, [draft, dirty, note.id, t]);

  // Leaving the note (another note, other tab, panel closed) saves what is pending.
  useEffect(
    () => () => {
      const { draft: d, dirty: pending } = latest.current;
      const current = storedNote(note.id);
      if (pending && current && (d.title.trim() || d.body.trim())) {
        void saveNote(toInput(current, d)).catch(() => undefined);
      }
    },
    [note.id],
  );

  const edit = (patch: Partial<Draft>) => {
    setDraft((d) => ({ ...d, ...patch }));
    setDirty(true);
    setStatus("idle");
  };

  const quick = async (patch: Partial<NoteInput>) => {
    const current = storedNote(note.id);
    if (!current) return;
    try {
      // Pending text edits go along, so they are not overwritten by the older stored text.
      const sent = latest.current;
      await saveNote(toInput(current, { ...(sent.dirty ? sent.draft : {}), ...patch }));
      if (sent.dirty && latest.current.draft === sent.draft) {
        latest.current.dirty = false;
        setDirty(false);
      }
    } catch (e) {
      setError(t("notes.saveFailed", { error: errorMessage(e) }));
    }
  };

  const linkedOpen = draft.repoPath
    ? openTabs.some((tab) => samePath(tab.path, draft.repoPath!))
    : false;
  const date = (ms: number) =>
    new Date(ms).toLocaleString(i18n.language, { dateStyle: "medium", timeStyle: "short" });

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 text-xs">
      <div className="flex items-center gap-1">
        <input
          value={draft.title}
          maxLength={200}
          placeholder={t("notes.titlePlaceholder")}
          aria-label={t("notes.titleLabel")}
          onChange={(e) => edit({ title: e.target.value })}
          className={cn(
            "h-8 min-w-0 flex-1 rounded-md border bg-background px-2 text-sm font-medium focus:border-primary focus:outline-none",
            note.done && "text-muted-foreground line-through",
          )}
        />
        <Button
          size="icon-sm"
          variant={note.pinned ? "default" : "outline"}
          aria-pressed={note.pinned}
          aria-label={note.pinned ? t("notes.unpin") : t("notes.pin")}
          title={note.pinned ? t("notes.unpin") : t("notes.pin")}
          onClick={() => void quick({ pinned: !note.pinned })}
        >
          <Pin />
        </Button>
        <Button
          size="icon-sm"
          variant={note.done ? "default" : "outline"}
          aria-pressed={note.done}
          aria-label={note.done ? t("notes.markOpen") : t("notes.markDone")}
          title={note.done ? t("notes.markOpen") : t("notes.markDone")}
          onClick={() => void quick({ done: !note.done })}
        >
          <CheckSquare />
        </Button>
      </div>

      <textarea
        value={draft.body}
        placeholder={t("notes.bodyPlaceholder")}
        aria-label={t("notes.bodyLabel")}
        maxLength={20000}
        onChange={(e) => edit({ body: e.target.value })}
        className="min-h-32 flex-1 resize-none rounded-md border bg-background px-2 py-1.5 text-xs leading-relaxed focus:border-primary focus:outline-none"
      />

      <div className="grid gap-1">
        <span className="text-muted-foreground">{t("notes.tags")}</span>
        <div className="flex flex-wrap gap-1">
          {tags.map((tag) => {
            const on = draft.tags.includes(tag.id);
            return (
              <button
                key={`tag-${tag.id}`}
                type="button"
                aria-pressed={on}
                onClick={() =>
                  edit({
                    tags: on ? draft.tags.filter((x) => x !== tag.id) : [...draft.tags, tag.id],
                  })
                }
                className="rounded-full focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              >
                <TagPill color={tag.color} muted={!on}>
                  {tagName(t, tag)}
                </TagPill>
              </button>
            );
          })}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2">
        <label className="grid gap-1">
          <span className="text-muted-foreground">{t("notes.priority")}</span>
          <Select
            value={draft.priority}
            onValueChange={(v) => edit({ priority: v as Draft["priority"] })}
          >
            <SelectTrigger className="h-8 w-full text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {PRIORITIES.map((p) => (
                <SelectItem key={`p-${p}`} value={p} className="text-xs">
                  {t(`notes.priorityLevel.${p}`)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </label>
        <label className="grid gap-1">
          <span className="text-muted-foreground">{t("notes.due")}</span>
          <span className="flex items-center gap-1">
            <input
              type="date"
              value={draft.due ?? ""}
              onChange={(e) => edit({ due: e.target.value || null })}
              className="h-8 min-w-0 flex-1 rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none"
            />
            {draft.due && (
              <Button
                size="icon-xs"
                variant="ghost"
                aria-label={t("notes.clearDue")}
                onClick={() => edit({ due: null })}
              >
                <X />
              </Button>
            )}
          </span>
        </label>
      </div>

      <label className="grid gap-1">
        <span className="text-muted-foreground">{t("notes.repo")}</span>
        <span className="flex items-center gap-1">
          <Select
            value={draft.repoPath ?? NO_REPO}
            onValueChange={(v) => {
              if (v === NO_REPO) return edit({ repoPath: null, repoName: null });
              const tab = openTabs.find((x) => x.path === v);
              edit({ repoPath: v, repoName: tab?.name ?? draft.repoName });
            }}
          >
            <SelectTrigger className="h-8 min-w-0 flex-1 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={NO_REPO} className="text-xs">
                {t("notes.noRepo")}
              </SelectItem>
              {draft.repoPath && !linkedOpen && (
                <SelectItem value={draft.repoPath} className="text-xs">
                  {t("notes.repoNotOpen", { name: draft.repoName ?? draft.repoPath })}
                </SelectItem>
              )}
              {openTabs.map((tab) => (
                <SelectItem key={`repo-${tab.id}`} value={tab.path} className="text-xs">
                  {tab.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {draft.repoPath && (
            <Button
              size="sm"
              variant="outline"
              className="h-8 text-xs"
              title={draft.repoPath}
              onClick={() => onOpenRepo(draft.repoPath!)}
            >
              <FolderGit2 />
              {linkedOpen ? t("notes.goToRepo") : t("notes.openRepo")}
            </Button>
          )}
        </span>
      </label>

      {error && (
        <p role="alert" className="text-destructive">
          {error}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
        <span>
          {t("notes.createdAt", { date: date(note.createdAt) })} ·{" "}
          {t("notes.updatedAt", { date: date(note.updatedAt) })}
        </span>
        <span className="flex-1" />
        <span aria-live="polite">
          {status === "saving"
            ? t("notes.saving")
            : dirty
              ? t("notes.unsaved")
              : status === "saved"
                ? t("notes.saved")
                : ""}
        </span>
        <Button size="xs" variant="ghost" className="text-destructive" onClick={onDelete}>
          <Trash2 />
          {t("notes.delete")}
        </Button>
      </div>
    </div>
  );
}
