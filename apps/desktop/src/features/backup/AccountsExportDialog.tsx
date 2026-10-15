import { useState } from "react";
import { Download, LoaderCircle } from "lucide-react";
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
import { AccountAvatar } from "@/features/accounts/AccountAvatar";
import { useAccounts, useProviders } from "@/features/accounts/queries";
import { Callout } from "@/features/accounts/manager/ui";
import { commands, errorMessage } from "@/ipc";
import { useUi } from "@/stores/ui";
import { pickSavePath } from "./files";
import { passwordProblem, toggleKey } from "./model";
import { CheckRow, NewPasswordFields, SecretsWarning } from "./parts";
import { useBackupUi } from "./store";

/** Phase 6.3: Account manager → Accounts → "Export configuration…". */
export function AccountsExportDialog() {
  const open = useBackupUi((s) => s.dialog === "accountsExport");
  const close = useBackupUi((s) => s.close);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent className="sm:max-w-lg">{open && <ExportForm onDone={close} />}</DialogContent>
    </Dialog>
  );
}

function ExportForm({ onDone }: { onDone: () => void }) {
  const { t } = useTranslation();
  const accounts = useAccounts().data ?? [];
  const providers = useProviders().data ?? [];
  /** `null` = every account (the default), else the ticked ids. */
  const [picked, setPicked] = useState<string[] | null>(null);
  const [withSecrets, setWithSecrets] = useState(false);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const order = accounts.map((a) => a.id);
  const selected = picked ?? order;
  const ready = selected.length > 0 && passwordProblem(password, confirm) === null;

  const submit = async () => {
    setError(null);
    const path = await pickSavePath(t, "accounts");
    if (!path) return;
    setBusy(true);
    try {
      const res = await commands.backupAccountsExport(path, password, selected, withSecrets);
      useUi.getState().showNotice({
        kind: "success",
        message: t("backup.accountsExport.done", { count: res.accounts, path: res.path }),
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
        <DialogTitle>{t("backup.accountsExport.title")}</DialogTitle>
        <DialogDescription>{t("backup.accountsExport.description")}</DialogDescription>
      </DialogHeader>

      <div className="grid gap-3">
        <div className="flex items-center justify-between text-xs">
          <span className="font-medium">{t("backup.accountsExport.pick")}</span>
          <Button
            size="xs"
            variant="ghost"
            onClick={() => setPicked(selected.length === order.length ? [] : null)}
          >
            {selected.length === order.length
              ? t("backup.accountsExport.none")
              : t("backup.accountsExport.all")}
          </Button>
        </div>
        <ul className="grid max-h-56 gap-1 overflow-y-auto rounded-md border p-1.5">
          {accounts.map((a) => {
            const provider = providers.find((p) => p.id === a.providerId);
            return (
              <li key={a.id}>
                <label className="flex cursor-pointer items-center gap-2 rounded px-1.5 py-1 text-xs hover:bg-muted/60">
                  <input
                    type="checkbox"
                    className="accent-primary"
                    checked={selected.includes(a.id)}
                    onChange={() => setPicked(toggleKey(selected, a.id, order))}
                  />
                  <AccountAvatar account={a} size={20} />
                  <span className="min-w-0 flex-1 truncate">
                    {a.label ?? a.displayName ?? a.username}
                    <span className="text-muted-foreground">
                      {" "}
                      · {provider?.name ?? a.providerId} · @{a.username}
                    </span>
                  </span>
                </label>
              </li>
            );
          })}
        </ul>
        {accounts.length > 0 && selected.length === 0 && (
          <p className="text-[11px] text-destructive">{t("backup.accountsExport.nothing")}</p>
        )}

        <CheckRow
          checked={withSecrets}
          onChange={setWithSecrets}
          label={t("backup.accountsExport.includeSecrets")}
          hint={t("backup.accountsExport.includeSecretsHint")}
          disabled={busy}
        />
        {withSecrets && <SecretsWarning />}

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
        <Button size="sm" disabled={!ready || busy} onClick={() => void submit()}>
          {busy ? <LoaderCircle className="animate-spin" /> : <Download />}
          {t("backup.accountsExport.submit")}
        </Button>
      </DialogFooter>
    </>
  );
}
