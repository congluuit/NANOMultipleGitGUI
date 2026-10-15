import { useState } from "react";
import { LoaderCircle, LockOpen, Upload } from "lucide-react";
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
import { refreshAccounts, refreshHealth } from "@/features/accounts/manager/queries";
import { Badge, Callout, Field, TextInput } from "@/features/accounts/manager/ui";
import { commands, errorMessage, type AccountsPreview, type BackupProblem } from "@/ipc";
import { useUi } from "@/stores/ui";
import { pickOpenPath } from "./files";
import { defaultSelection, importSummary, toggleKey } from "./model";
import { CheckRow, FileRow, ProblemBox } from "./parts";
import { useBackupUi } from "./store";

/** Phase 6.3: Account manager → Accounts → "Import configuration…". */
export function AccountsImportDialog() {
  const open = useBackupUi((s) => s.dialog === "accountsImport");
  const close = useBackupUi((s) => s.close);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent className="sm:max-w-lg">{open && <ImportForm onDone={close} />}</DialogContent>
    </Dialog>
  );
}

function ImportForm({ onDone }: { onDone: () => void }) {
  const { t } = useTranslation();
  const [path, setPath] = useState<string | null>(null);
  const [password, setPassword] = useState("");
  const [preview, setPreview] = useState<AccountsPreview | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [updateExisting, setUpdateExisting] = useState(false);
  const [problem, setProblem] = useState<BackupProblem | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reset = () => {
    setPreview(null);
    setProblem(null);
    setError(null);
  };

  const pick = async () => {
    const chosen = await pickOpenPath(t, "accounts");
    if (chosen) {
      setPath(chosen);
      reset();
    }
  };

  const openFile = async () => {
    if (!path) return;
    reset();
    setBusy(true);
    try {
      const res = await commands.backupAccountsOpen(path, password);
      setProblem(res.problem);
      if (res.preview) {
        setPreview(res.preview);
        setSelected(defaultSelection(res.preview.accounts));
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const apply = async () => {
    if (!path) return;
    setError(null);
    setBusy(true);
    try {
      const res = await commands.backupAccountsImport(path, password, selected, updateExisting);
      if (res.problem) {
        setProblem(res.problem);
        return;
      }
      refreshAccounts();
      refreshHealth();
      if (res.report) {
        const summary = importSummary(t, res.report);
        useUi.getState().showNotice({
          kind: res.report.needSignIn.length > 0 ? "info" : "success",
          message: summary.message,
          hint: summary.lines.join("\n") || undefined,
        });
      }
      onDone();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const order = preview?.accounts.map((a) => a.key) ?? [];

  return (
    <>
      <DialogHeader>
        <DialogTitle>{t("backup.accountsImport.title")}</DialogTitle>
        <DialogDescription>{t("backup.accountsImport.description")}</DialogDescription>
      </DialogHeader>

      {!preview ? (
        <form
          className="grid gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            void openFile();
          }}
        >
          <FileRow path={path} onPick={() => void pick()} disabled={busy} />
          <Field label={t("backup.password.openLabel")}>
            <TextInput
              type="password"
              autoComplete="off"
              value={password}
              disabled={busy}
              onChange={(e) => setPassword(e.target.value)}
            />
          </Field>
          {problem && <ProblemBox problem={problem} kind="accounts" />}
          {error && <Callout tone="danger">{error}</Callout>}
          <DialogFooter>
            <Button type="button" variant="outline" size="sm" onClick={onDone}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={!path || !password || busy}>
              {busy ? <LoaderCircle className="animate-spin" /> : <LockOpen />}
              {t("backup.accountsImport.open")}
            </Button>
          </DialogFooter>
        </form>
      ) : (
        <div className="grid gap-3">
          {preview.hasSecrets && (
            <p className="text-[11px] text-muted-foreground">
              {t("backup.accountsImport.hasSecrets")}
            </p>
          )}
          <ul className="grid max-h-64 gap-1 overflow-y-auto rounded-md border p-1.5">
            {preview.accounts.map((a) => (
              <li key={a.key}>
                <label className="flex cursor-pointer items-start gap-2 rounded px-1.5 py-1 text-xs hover:bg-muted/60">
                  <input
                    type="checkbox"
                    className="mt-0.5 accent-primary"
                    checked={selected.includes(a.key)}
                    onChange={() => setSelected(toggleKey(selected, a.key, order))}
                  />
                  <span
                    aria-hidden
                    className="mt-1 size-2 shrink-0 rounded-full"
                    style={{ background: a.color }}
                  />
                  <span className="grid min-w-0 flex-1 gap-0.5">
                    <span className="truncate font-medium">
                      {a.label ?? a.displayName ?? a.username}
                      <span className="font-normal text-muted-foreground">
                        {" "}
                        · {a.providerName} ({a.host}) · @{a.username}
                      </span>
                    </span>
                    <span className="flex flex-wrap items-center gap-1 text-[11px] text-muted-foreground">
                      {a.existingAccountId && (
                        <Badge tone="warn">{t("backup.accountsImport.exists")}</Badge>
                      )}
                      {a.newProvider && <Badge>{t("backup.accountsImport.newProvider")}</Badge>}
                      {a.authMethod !== "sshOnly" &&
                        (a.hasToken ? (
                          <Badge tone="ok">{t("backup.accountsImport.hasToken")}</Badge>
                        ) : (
                          <Badge>{t("backup.accountsImport.noToken")}</Badge>
                        ))}
                      <span>
                        {t("backup.accountsImport.counts", {
                          identities: a.identities,
                          rules: a.rules,
                        })}
                      </span>
                    </span>
                  </span>
                </label>
              </li>
            ))}
          </ul>
          {preview.accounts.some((a) => a.existingAccountId) && (
            <CheckRow
              checked={updateExisting}
              onChange={setUpdateExisting}
              label={t("backup.accountsImport.updateExisting")}
              hint={t("backup.accountsImport.updateExistingHint")}
              disabled={busy}
            />
          )}
          {problem && <ProblemBox problem={problem} kind="accounts" />}
          {error && <Callout tone="danger">{error}</Callout>}
          <DialogFooter>
            <Button variant="outline" size="sm" disabled={busy} onClick={reset}>
              {t("backup.accountsImport.back")}
            </Button>
            <Button size="sm" disabled={selected.length === 0 || busy} onClick={() => void apply()}>
              {busy ? <LoaderCircle className="animate-spin" /> : <Upload />}
              {t("backup.accountsImport.submit", { count: selected.length })}
            </Button>
          </DialogFooter>
        </div>
      )}
    </>
  );
}
