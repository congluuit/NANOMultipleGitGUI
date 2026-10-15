import { useState } from "react";
import { History, LoaderCircle, LockOpen, TriangleAlert } from "lucide-react";
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
import { Callout, Field, TextInput } from "@/features/accounts/manager/ui";
import {
  commands,
  errorMessage,
  type BackupInfo,
  type BackupProblem,
  type FullPreview,
} from "@/ipc";
import { pickOpenPath } from "./files";
import { FileRow, ProblemBox } from "./parts";
import { useBackupUi } from "./store";

/** Phase 6.6: Settings → Backup & restore → "Restore from backup…". */
export function FullRestoreDialog() {
  const open = useBackupUi((s) => s.dialog === "fullRestore");
  const close = useBackupUi((s) => s.close);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent className="sm:max-w-lg">
        {open && <RestoreForm onDone={close} />}
      </DialogContent>
    </Dialog>
  );
}

function RestoreForm({ onDone }: { onDone: () => void }) {
  const { t, i18n } = useTranslation();
  const [path, setPath] = useState<string | null>(null);
  const [password, setPassword] = useState("");
  const [opened, setOpened] = useState<{ info: BackupInfo; preview: FullPreview } | null>(null);
  const [problem, setProblem] = useState<BackupProblem | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"open" | "restore" | null>(null);
  const [confirming, setConfirming] = useState(false);

  const reset = () => {
    setOpened(null);
    setProblem(null);
    setError(null);
  };

  const pick = async () => {
    const chosen = await pickOpenPath(t, "full");
    if (chosen) {
      setPath(chosen);
      reset();
    }
  };

  const openFile = async () => {
    if (!path) return;
    reset();
    setBusy("open");
    try {
      const res = await commands.backupFullOpen(path, password);
      setProblem(res.problem);
      if (res.info && res.preview) setOpened({ info: res.info, preview: res.preview });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  const restore = async () => {
    if (!path) return;
    setConfirming(false);
    setError(null);
    setBusy("restore");
    try {
      const res = await commands.backupFullRestore(path, password);
      if (res.problem) {
        setProblem(res.problem);
        setBusy(null);
        return;
      }
      // The staged database is swapped in while the app starts again.
      await commands.storageRestart();
    } catch (e) {
      setError(errorMessage(e));
      setBusy(null);
    }
  };

  const created = opened ? new Date(opened.info.createdAt).toLocaleString(i18n.language) : "";
  const p = opened?.preview;

  return (
    <>
      <DialogHeader>
        <DialogTitle>{t("backup.full.restoreTitle")}</DialogTitle>
        <DialogDescription>{t("backup.full.restoreDescription")}</DialogDescription>
      </DialogHeader>

      {!opened || !p ? (
        <form
          className="grid gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            void openFile();
          }}
        >
          <FileRow path={path} onPick={() => void pick()} disabled={busy !== null} />
          <Field label={t("backup.password.openLabel")}>
            <TextInput
              type="password"
              autoComplete="off"
              value={password}
              disabled={busy !== null}
              onChange={(e) => setPassword(e.target.value)}
            />
          </Field>
          {problem && <ProblemBox problem={problem} kind="full" />}
          {error && <Callout tone="danger">{error}</Callout>}
          <DialogFooter>
            <Button type="button" variant="outline" size="sm" onClick={onDone}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={!path || !password || busy !== null}>
              {busy === "open" ? <LoaderCircle className="animate-spin" /> : <LockOpen />}
              {t("backup.accountsImport.open")}
            </Button>
          </DialogFooter>
        </form>
      ) : (
        <div className="grid gap-3 text-xs">
          <div className="grid gap-1 rounded-md border p-3">
            <p className="font-medium">
              {t("backup.full.created", { date: created, version: opened.info.appVersion })}
            </p>
            <ul className="list-inside list-disc text-muted-foreground">
              <li>{t("backup.full.accounts", { count: p.accounts })}</li>
              <li>{t("backup.full.repos", { count: p.repos })}</li>
              {p.notes !== null && <li>{t("backup.full.notes", { count: p.notes })}</li>}
              {p.customThemes !== null && (
                <li>{t("backup.full.themes", { count: p.customThemes })}</li>
              )}
              <li>
                {p.includesSecrets
                  ? t("backup.full.withSecrets", { count: p.secrets, keys: p.sshKeyFiles })
                  : t("backup.full.withoutSecrets")}
              </li>
            </ul>
          </div>
          <p className="text-[11px] text-muted-foreground">{t("backup.full.reposNote")}</p>
          <Callout tone="warn">
            <p className="flex items-start gap-1.5">
              <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
              <span>{t("backup.full.replaceWarning")}</span>
            </p>
          </Callout>
          {problem && <ProblemBox problem={problem} kind="full" />}
          {error && <Callout tone="danger">{error}</Callout>}
          <DialogFooter>
            <Button variant="outline" size="sm" disabled={busy !== null} onClick={reset}>
              {t("backup.accountsImport.back")}
            </Button>
            <Button
              size="sm"
              variant="destructive"
              disabled={busy !== null}
              onClick={() => setConfirming(true)}
            >
              {busy === "restore" ? <LoaderCircle className="animate-spin" /> : <History />}
              {busy === "restore" ? t("backup.full.restoring") : t("backup.full.confirm")}
            </Button>
          </DialogFooter>
          <ConfirmDialog
            open={confirming}
            title={t("backup.full.confirmTitle")}
            description={t("backup.full.replaceWarning")}
            confirmLabel={t("backup.full.confirm")}
            destructive
            onCancel={() => setConfirming(false)}
            onConfirm={() => void restore()}
          />
        </div>
      )}
    </>
  );
}
