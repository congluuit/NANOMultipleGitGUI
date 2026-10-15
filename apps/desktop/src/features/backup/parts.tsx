import { FolderOpen, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Callout, Field, TextInput } from "@/features/accounts/manager/ui";
import type { BackupKind, BackupProblem } from "@/ipc";
import { passwordProblem, passwordProblemText, problemText } from "./model";

/** Password + confirmation for a new file. Errors show once something was typed. */
export function NewPasswordFields({
  password,
  confirm,
  onPassword,
  onConfirm,
  disabled,
}: {
  password: string;
  confirm: string;
  onPassword: (v: string) => void;
  onConfirm: (v: string) => void;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const problem = passwordProblem(password, confirm);
  const tooShort = password.length > 0 && problem === "tooShort";
  const mismatch = confirm.length > 0 && problem === "mismatch";
  return (
    <div className="grid grid-cols-2 gap-3">
      <Field
        label={t("backup.password.label")}
        error={tooShort ? passwordProblemText(t, "tooShort") : undefined}
      >
        <TextInput
          type="password"
          autoComplete="new-password"
          value={password}
          disabled={disabled}
          aria-invalid={tooShort || undefined}
          onChange={(e) => onPassword(e.target.value)}
        />
      </Field>
      <Field
        label={t("backup.password.confirm")}
        error={mismatch ? passwordProblemText(t, "mismatch") : undefined}
      >
        <TextInput
          type="password"
          autoComplete="new-password"
          value={confirm}
          disabled={disabled}
          aria-invalid={mismatch || undefined}
          onChange={(e) => onConfirm(e.target.value)}
        />
      </Field>
      <p className="col-span-2 text-[11px] text-muted-foreground">{t("backup.password.hint")}</p>
    </div>
  );
}

/** Chosen file + "Choose file…". */
export function FileRow({
  path,
  onPick,
  disabled,
}: {
  path: string | null;
  onPick: () => void;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-2 text-xs">
      <span
        className="min-w-0 flex-1 truncate rounded-md border bg-muted/40 px-2 py-1.5 font-mono text-[11px]"
        title={path ?? undefined}
      >
        {path ?? <span className="font-sans text-muted-foreground">{t("backup.file.none")}</span>}
      </span>
      <Button size="sm" variant="outline" disabled={disabled} onClick={onPick}>
        <FolderOpen />
        {t("backup.file.choose")}
      </Button>
    </div>
  );
}

export function ProblemBox({ problem, kind }: { problem: BackupProblem; kind: BackupKind }) {
  const { t } = useTranslation();
  const text = problemText(t, problem, kind);
  return (
    <Callout tone="danger">
      <p role="alert" className="font-medium">
        {text.message}
      </p>
      {text.hint && <p className="mt-1">{text.hint}</p>}
    </Callout>
  );
}

/** Shown while "include sign-in data" is ticked. */
export function SecretsWarning() {
  const { t } = useTranslation();
  return (
    <Callout tone="warn">
      <p className="flex items-start gap-1.5">
        <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
        <span>{t("backup.secrets.keepSafe")}</span>
      </p>
      <p className="mt-1">{t("backup.secrets.oauthWarning")}</p>
    </Callout>
  );
}

export function CheckRow({
  checked,
  onChange,
  label,
  hint,
  disabled,
}: {
  checked: boolean;
  onChange: (on: boolean) => void;
  label: string;
  hint?: string;
  disabled?: boolean;
}) {
  return (
    <label className="flex items-start gap-3 rounded-md border p-3 text-xs">
      <input
        type="checkbox"
        className="mt-0.5 accent-primary"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="grid gap-0.5">
        <span className="font-medium">{label}</span>
        {hint && <span className="text-[11px] text-muted-foreground">{hint}</span>}
      </span>
    </label>
  );
}
