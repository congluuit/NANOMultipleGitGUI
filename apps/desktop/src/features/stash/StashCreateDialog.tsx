import { useState } from "react";
import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { createStash, DEFAULT_STASH_FORM, type StashForm } from "./actions";
import { useStashUi } from "./store";

interface Props {
  open: boolean;
  busy?: boolean;
  onSubmit: (form: StashForm) => void;
  onCancel: () => void;
}

/** "Stash changes" form: optional message, untracked files, keep the index. */
export function StashCreateDialog({ open, busy, onSubmit, onCancel }: Props) {
  const { t } = useTranslation();
  const [form, setForm] = useState<StashForm>(DEFAULT_STASH_FORM);
  const set = (patch: Partial<StashForm>) => setForm((f) => ({ ...f, ...patch }));

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onCancel()}>
      <DialogContent className="sm:max-w-md">
        <form
          className="grid gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (!busy) onSubmit(form);
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("stash.create")}</DialogTitle>
            <DialogDescription>{t("stash.createDesc")}</DialogDescription>
          </DialogHeader>
          <label className="grid gap-1.5 text-xs">
            <span className="text-muted-foreground">{t("stash.message")}</span>
            <input
              value={form.message}
              onChange={(e) => set({ message: e.target.value })}
              placeholder={t("stash.messagePlaceholder")}
              autoFocus
              className="h-8 rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none"
            />
          </label>
          <div className="grid gap-2 text-xs">
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={form.includeUntracked}
                onChange={(e) => set({ includeUntracked: e.target.checked })}
                className="accent-primary"
              />
              {t("stash.includeUntracked")}
            </label>
            <label className="flex items-center gap-2" title={t("stash.keepIndexHint")}>
              <input
                type="checkbox"
                checked={form.keepIndex}
                onChange={(e) => set({ keepIndex: e.target.checked })}
                className="accent-primary"
              />
              {t("stash.keepIndex")}
            </label>
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" size="sm" onClick={onCancel}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={busy}>
              {busy && <LoaderCircle className="animate-spin" />}
              {t("stash.submit")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/** The dialog wired to `useStashUi` and `stash_create` (mounted once, in the toolbar). */
export function StashCreateDialogHost() {
  const repoId = useStashUi((s) => s.createFor);
  const close = useStashUi((s) => s.closeCreate);
  const [busy, setBusy] = useState(false);

  const submit = async (form: StashForm) => {
    if (repoId === null) return;
    setBusy(true);
    const result = await createStash(repoId, form);
    setBusy(false);
    // Keep the form open on failure so the message is not lost.
    if (result !== "failed") close();
  };

  return (
    <StashCreateDialog
      // A fresh form every time the dialog opens.
      key={repoId ?? "closed"}
      open={repoId !== null}
      busy={busy}
      onSubmit={(form) => void submit(form)}
      onCancel={close}
    />
  );
}
