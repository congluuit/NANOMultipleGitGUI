import { useState } from "react";
import { HardDriveDownload, LoaderCircle } from "lucide-react";
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
import { Callout } from "@/features/accounts/manager/ui";
import { commands, errorMessage } from "@/ipc";
import { formatBytes } from "@/lib/format";
import { useUi } from "@/stores/ui";
import { pickSavePath } from "./files";
import { passwordProblem } from "./model";
import { CheckRow, NewPasswordFields, SecretsWarning } from "./parts";
import { useBackupUi } from "./store";

/** Phase 6.6: Settings → Backup & restore → "Create backup…". */
export function FullBackupDialog() {
  const open = useBackupUi((s) => s.dialog === "fullCreate");
  const close = useBackupUi((s) => s.close);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent className="sm:max-w-lg">{open && <CreateForm onDone={close} />}</DialogContent>
    </Dialog>
  );
}

function CreateForm({ onDone }: { onDone: () => void }) {
  const { t } = useTranslation();
  // On by default: the point of a full backup is "restore and keep working".
  const [withSecrets, setWithSecrets] = useState(true);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    setError(null);
    const path = await pickSavePath(t, "full");
    if (!path) return;
    setBusy(true);
    try {
      const res = await commands.backupFullCreate(path, password, withSecrets);
      useUi.getState().showNotice({
        kind: "info",
        message: t("backup.full.done", { size: formatBytes(res.bytes), path: res.path }),
        hint: t("backup.full.doneHint"),
      });
      onDone();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <DialogHeader>
        <DialogTitle>{t("backup.full.createTitle")}</DialogTitle>
        <DialogDescription>{t("backup.full.createDescription")}</DialogDescription>
      </DialogHeader>
      <div className="grid gap-3">
        <CheckRow
          checked={withSecrets}
          onChange={setWithSecrets}
          label={t("backup.full.includeSecrets")}
          hint={t("backup.full.includeSecretsHint")}
          disabled={busy}
        />
        {withSecrets && <SecretsWarning />}
        <p className="text-[11px] text-muted-foreground">{t("backup.full.notIncluded")}</p>
        <NewPasswordFields
          password={password}
          confirm={confirm}
          onPassword={setPassword}
          onConfirm={setConfirm}
          disabled={busy}
        />
        {error && <Callout tone="danger">{error}</Callout>}
      </div>
      <DialogFooter>
        <Button variant="outline" size="sm" onClick={onDone}>
          {t("common.cancel")}
        </Button>
        <Button
          size="sm"
          disabled={busy || passwordProblem(password, confirm) !== null}
          onClick={() => void submit()}
        >
          {busy ? <LoaderCircle className="animate-spin" /> : <HardDriveDownload />}
          {t("backup.full.submit")}
        </Button>
      </DialogFooter>
    </>
  );
}
