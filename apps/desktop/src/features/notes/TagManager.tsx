// Phase 6.5: the user's own note tags (built-in tags are fixed).
import { useState } from "react";
import { Check, Pencil, Plus, Trash2, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { commands, errorMessage, type NoteTag } from "@/ipc";
import { cn } from "@/lib/utils";
import { TAG_COLORS, allTags, tagName } from "./model";
import { setNotes, useNotes } from "./queries";
import { TagPill } from "./TagPill";

interface Editing {
  id: string | null;
  name: string;
  color: string;
}

export function TagManager({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const data = useNotes().data;
  const userTags = data?.tags ?? [];
  const notes = data?.notes ?? [];
  const [editing, setEditing] = useState<Editing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [deleting, setDeleting] = useState<NoteTag | null>(null);

  const save = async () => {
    if (!editing) return;
    setBusy(true);
    setError(null);
    try {
      setNotes(
        await commands.notesTagSave({
          id: editing.id,
          name: editing.name,
          color: editing.color,
        }),
      );
      setEditing(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!deleting) return;
    setBusy(true);
    try {
      setNotes(await commands.notesTagDelete(deleting.id));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
      setDeleting(null);
    }
  };

  const usage = (id: string) => notes.filter((n) => n.tags.includes(id)).length;

  return (
    <>
      <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
        <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("notes.tagManager.title")}</DialogTitle>
            <DialogDescription>{t("notes.tagManager.description")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-3 text-xs">
            <div>
              <div className="mb-1 font-medium">{t("notes.tagManager.builtin")}</div>
              <div className="flex flex-wrap gap-1">
                {allTags([]).map((tag) => (
                  <TagPill key={`b-${tag.id}`} color={tag.color}>
                    {tagName(t, tag)}
                  </TagPill>
                ))}
              </div>
            </div>
            <div>
              <div className="mb-1 font-medium">{t("notes.tagManager.yours")}</div>
              {userTags.length === 0 && !editing && (
                <p className="text-muted-foreground">{t("notes.tagManager.none")}</p>
              )}
              <ul className="space-y-1">
                {userTags.map((tag) =>
                  editing?.id === tag.id ? null : (
                    <li key={`u-${tag.id}`} className="flex items-center gap-2">
                      <TagPill color={tag.color}>{tag.name}</TagPill>
                      <span className="flex-1 text-[11px] text-muted-foreground">
                        {t("notes.tagManager.used", { count: usage(tag.id) })}
                      </span>
                      <Button
                        size="icon-xs"
                        variant="ghost"
                        aria-label={t("notes.tagManager.edit", { name: tag.name })}
                        onClick={() => setEditing({ ...tag })}
                      >
                        <Pencil />
                      </Button>
                      <Button
                        size="icon-xs"
                        variant="ghost"
                        className="text-destructive"
                        aria-label={t("notes.tagManager.delete", { name: tag.name })}
                        onClick={() => setDeleting(tag)}
                      >
                        <Trash2 />
                      </Button>
                    </li>
                  ),
                )}
              </ul>
            </div>

            {editing ? (
              <div className="space-y-2 rounded-md border p-2">
                <label className="grid gap-1">
                  <span className="text-muted-foreground">{t("notes.tagManager.name")}</span>
                  <input
                    autoFocus
                    value={editing.name}
                    maxLength={40}
                    onChange={(e) => setEditing({ ...editing, name: e.target.value })}
                    onKeyDown={(e) => e.key === "Enter" && void save()}
                    className="h-8 rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none"
                  />
                </label>
                <div className="grid gap-1">
                  <span className="text-muted-foreground">{t("notes.tagManager.color")}</span>
                  <div className="flex flex-wrap gap-1" role="radiogroup">
                    {TAG_COLORS.map((c) => (
                      <button
                        key={`c-${c}`}
                        type="button"
                        role="radio"
                        aria-checked={editing.color === c}
                        aria-label={c}
                        onClick={() => setEditing({ ...editing, color: c })}
                        className={cn(
                          "flex size-6 items-center justify-center rounded-full border-2 border-transparent",
                          editing.color === c && "border-foreground",
                        )}
                        style={{ background: c }}
                      >
                        {editing.color === c && (
                          <Check className="size-3 text-white mix-blend-difference" />
                        )}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <TagPill color={editing.color}>
                    {editing.name.trim() || t("notes.tagManager.preview")}
                  </TagPill>
                  <span className="flex-1" />
                  <Button size="xs" variant="ghost" onClick={() => setEditing(null)}>
                    <X />
                    {t("common.cancel")}
                  </Button>
                  <Button
                    size="xs"
                    disabled={busy || !editing.name.trim()}
                    onClick={() => void save()}
                  >
                    <Check />
                    {t("notes.tagManager.save")}
                  </Button>
                </div>
              </div>
            ) : (
              <Button
                size="sm"
                variant="outline"
                className="h-8 w-full text-xs"
                onClick={() =>
                  setEditing({
                    id: null,
                    name: "",
                    color: TAG_COLORS[userTags.length % TAG_COLORS.length]!,
                  })
                }
              >
                <Plus />
                {t("notes.tagManager.add")}
              </Button>
            )}
            {error && (
              <p role="alert" className="text-destructive">
                {error}
              </p>
            )}
          </div>
        </DialogContent>
      </Dialog>
      <ConfirmDialog
        open={deleting !== null}
        title={t("notes.tagManager.deleteTitle", { name: deleting?.name ?? "" })}
        description={t("notes.tagManager.deleteBody", { count: deleting ? usage(deleting.id) : 0 })}
        confirmLabel={t("notes.tagManager.deleteConfirm")}
        destructive
        busy={busy}
        onConfirm={() => void remove()}
        onCancel={() => setDeleting(null)}
      />
    </>
  );
}
