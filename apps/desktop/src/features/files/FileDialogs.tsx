import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cleanPath, createFile, createFolder, deletePath, joinPath, renamePath } from "./actions";
import { useWorkspace } from "@/stores/workspace";
import {
  isSearchShortcut,
  openFileDialog,
  showSidebarView,
  useFileDialog,
  type FileDialog,
} from "./store";
import { baseName } from "./tree";

/** The file tree's dialogs and the Ctrl+Shift+F shortcut (search panel). Mount once. */
export function FilesHost() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!isSearchShortcut(e) || useWorkspace.getState().activeId === null) return;
      if (e.target instanceof Element && e.target.closest(".xterm")) return;
      e.preventDefault();
      e.stopPropagation();
      showSidebarView("search");
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
  return <FileDialogs />;
}

/** Create / rename / delete dialogs of the file tree. */
function FileDialogs() {
  const dialog = useFileDialog((s) => s.dialog);
  if (!dialog) return null;
  const key = JSON.stringify(dialog);
  const close = () => openFileDialog(null);
  return dialog.kind === "delete" ? (
    <DeleteDialog key={key} dialog={dialog} onClose={close} />
  ) : (
    <NameDialog key={key} dialog={dialog} onClose={close} />
  );
}

function NameDialog({
  dialog,
  onClose,
}: {
  dialog: Exclude<FileDialog, { kind: "delete" }>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const from = dialog.kind === "rename" ? dialog.path : null;
  const rename = from !== null;
  const [value, setValue] = useState(from ?? "");
  const [busy, setBusy] = useState(false);
  const target = dialog.kind === "rename" ? cleanPath(value) : joinPath(dialog.parent, value);
  const valid = cleanPath(value) !== "" && target !== from;

  const submit = async () => {
    if (!valid || busy) return;
    setBusy(true);
    const ok =
      dialog.kind === "rename"
        ? await renamePath(dialog.repoId, dialog.path, target)
        : dialog.kind === "newFile"
          ? await createFile(dialog.repoId, target)
          : await createFolder(dialog.repoId, target);
    setBusy(false);
    if (ok) onClose();
  };

  const title =
    dialog.kind === "rename"
      ? t(dialog.folder ? "files.renameFolderTitle" : "files.renameFileTitle", {
          name: baseName(dialog.path),
        })
      : dialog.kind === "newFile"
        ? t("files.newFileTitle")
        : t("files.newFolderTitle");
  const where = dialog.kind === "rename" ? null : dialog.parent || t("files.root");

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription className="break-words">
            {rename ? t("files.renameHint") : t("files.createHint", { folder: where })}
          </DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          <input
            autoFocus
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onFocus={(e) => {
              // Renaming selects the name without its extension, as file managers do. After
              // a frame: the dialog's focus trap selects everything when it (re)focuses.
              if (from === null || e.currentTarget.value !== from) return;
              const el = e.currentTarget;
              const name = baseName(from);
              const start = from.length - name.length;
              const dot = name.lastIndexOf(".");
              requestAnimationFrame(() =>
                el.setSelectionRange(start, dot > 0 ? start + dot : from.length),
              );
            }}
            placeholder={dialog.kind === "newFolder" ? "folder" : "file.txt"}
            aria-label={title}
            className="h-8 w-full rounded-md border bg-background px-2 font-mono text-xs focus:border-primary focus:outline-none"
          />
        </form>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button size="sm" disabled={!valid || busy} onClick={() => void submit()}>
            {rename ? t("files.rename") : t("files.create")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function DeleteDialog({
  dialog,
  onClose,
}: {
  dialog: Extract<FileDialog, { kind: "delete" }>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const name = baseName(dialog.path);
  return (
    <ConfirmDialog
      open
      title={t(dialog.folder ? "files.deleteFolderTitle" : "files.deleteFileTitle", { name })}
      description={
        dialog.folder
          ? t("files.deleteFolderBody", { path: dialog.path })
          : dialog.untracked
            ? t("files.deleteUntrackedBody", { path: dialog.path })
            : t("files.deleteTrackedBody", { path: dialog.path })
      }
      confirmLabel={t("files.delete")}
      destructive
      busy={busy}
      onConfirm={() => {
        setBusy(true);
        void deletePath(dialog.repoId, dialog.path).then((ok) => {
          setBusy(false);
          if (ok) onClose();
        });
      }}
      onCancel={onClose}
    />
  );
}
