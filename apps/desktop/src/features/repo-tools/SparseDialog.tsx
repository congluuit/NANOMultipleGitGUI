import { useState } from "react";
import { Folder, LoaderCircle } from "lucide-react";
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
import { errorMessage, type SparseState } from "@/ipc";
import { disableSparse, setSparseFolders } from "./actions";
import { nestedFolders, parseFolderLines } from "./model";
import { useSparseState } from "./queries";

function SparseForm({
  repoId,
  state,
  onClose,
}: {
  repoId: number;
  state: SparseState;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [picked, setPicked] = useState<string[]>(() =>
    state.enabled ? state.folders.filter((f) => state.available.includes(f)) : state.available,
  );
  const [extra, setExtra] = useState(() => nestedFolders(state).join("\n"));
  const [busy, setBusy] = useState(false);
  const folders = [...picked, ...parseFolderLines(extra).filter((f) => !picked.includes(f))];
  const keepsAll =
    parseFolderLines(extra).length === 0 && state.available.every((f) => picked.includes(f));

  const toggle = (folder: string) =>
    setPicked((p) => (p.includes(folder) ? p.filter((f) => f !== folder) : [...p, folder]));
  const run = async (work: () => Promise<boolean>) => {
    setBusy(true);
    const ok = await work();
    setBusy(false);
    if (ok) onClose();
  };

  return (
    <>
      <div className="space-y-3">
        <div>
          <div className="mb-1 flex items-center justify-between">
            <span className="text-[11px] font-semibold text-muted-foreground uppercase">
              {t("sparse.folders")}
            </span>
            <span className="flex gap-1">
              <Button size="xs" variant="ghost" onClick={() => setPicked(state.available)}>
                {t("sparse.all")}
              </Button>
              <Button size="xs" variant="ghost" onClick={() => setPicked([])}>
                {t("sparse.none")}
              </Button>
            </span>
          </div>
          <ul className="max-h-[36vh] overflow-y-auto rounded-md border bg-background py-1">
            {state.available.map((folder) => (
              <li key={folder}>
                <label className="flex cursor-pointer items-center gap-2 px-2 py-1 text-xs hover:bg-accent">
                  <input
                    type="checkbox"
                    checked={picked.includes(folder)}
                    onChange={() => toggle(folder)}
                  />
                  <Folder className="size-3.5 text-muted-foreground" />
                  <span className="truncate">{folder}</span>
                </label>
              </li>
            ))}
            {state.available.length === 0 && (
              <li className="px-2 py-3 text-xs text-muted-foreground">{t("sparse.noFolders")}</li>
            )}
          </ul>
        </div>
        <label className="block">
          <span className="mb-1 block text-[11px] font-semibold text-muted-foreground uppercase">
            {t("sparse.extra")}
          </span>
          <textarea
            value={extra}
            onChange={(e) => setExtra(e.target.value)}
            rows={3}
            spellCheck={false}
            placeholder={t("sparse.extraPlaceholder")}
            className="w-full resize-y rounded-md border bg-background px-2 py-1 font-mono text-xs outline-none focus-visible:border-ring"
          />
        </label>
        <p className="text-[11px] text-muted-foreground">{t("sparse.note")}</p>
      </div>
      <DialogFooter>
        {state.enabled && (
          <Button
            size="sm"
            variant="outline"
            disabled={busy}
            className="sm:mr-auto"
            onClick={() => void run(() => disableSparse(repoId))}
          >
            {t("sparse.disable")}
          </Button>
        )}
        <Button size="sm" variant="outline" onClick={onClose}>
          {t("common.cancel")}
        </Button>
        <Button
          size="sm"
          disabled={busy || folders.length === 0 || (keepsAll && !state.enabled)}
          title={keepsAll && !state.enabled ? t("sparse.keepsAllTip") : undefined}
          onClick={() =>
            void run(() =>
              keepsAll && state.enabled ? disableSparse(repoId) : setSparseFolders(repoId, folders),
            )
          }
        >
          {busy && <LoaderCircle className="animate-spin" />}
          {t("sparse.apply", { count: folders.length })}
        </Button>
      </DialogFooter>
    </>
  );
}

/** Sparse checkout: keep only the chosen folders on disk. */
export function SparseDialog({ repoId, onClose }: { repoId: number; onClose: () => void }) {
  const { t } = useTranslation();
  const state = useSparseState(repoId, true);
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("sparse.title")}</DialogTitle>
          <DialogDescription>
            {state.data?.enabled
              ? t("sparse.descriptionOn", { count: state.data.folders.length })
              : t("sparse.description")}
          </DialogDescription>
        </DialogHeader>
        {state.isPending && (
          <p className="flex items-center gap-2 text-xs text-muted-foreground">
            <LoaderCircle className="size-4 animate-spin" />
            {t("tools.loading")}
          </p>
        )}
        {state.isError && <p className="text-xs text-danger">{errorMessage(state.error)}</p>}
        {state.data && <SparseForm repoId={repoId} state={state.data} onClose={onClose} />}
      </DialogContent>
    </Dialog>
  );
}
