import { GitFork, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Account } from "@/ipc";
import { accountLabel } from "@/features/accounts/switcher/model";
import { NamespaceSelect } from "./NamespaceSelect";
import { canOfferFork, type ForkPhase, type ForkSource } from "./model";

/** "Fork to ● account, then clone" choice of the clone dialog. */
export interface ForkChoice {
  enabled: boolean;
  /** `null` = the account's own namespace. */
  namespace: string | null;
}

/**
 * Checkbox + namespace for the clone dialog, shown when the chosen repository does not belong
 * to the chosen account. The flow itself is `forkThenClone`.
 */
export function CloneForkOption({
  source,
  account,
  value,
  onChange,
  disabled,
}: {
  source: ForkSource | null;
  account: Account | undefined;
  value: ForkChoice | null;
  onChange: (choice: ForkChoice) => void;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  if (!source || !account || !canOfferFork(source, account)) return null;
  const choice = value ?? { enabled: false, namespace: null };
  return (
    <>
      <span className="font-semibold">{t("forks.cloneLabel")}</span>
      <div className="space-y-1">
        <label className="flex items-center gap-1.5">
          <input
            type="checkbox"
            checked={choice.enabled}
            disabled={disabled}
            onChange={(e) => onChange({ ...choice, enabled: e.target.checked })}
          />
          <GitFork className="size-3.5 text-muted-foreground" />
          <span>
            {t("forks.cloneOption.before")}
            <span
              aria-hidden
              className="mx-1 inline-block size-2 rounded-full"
              style={{ backgroundColor: account.color }}
            />
            <span className="font-medium">{accountLabel(account)}</span>
            {t("forks.cloneOption.after")}
          </span>
        </label>
        {choice.enabled && (
          <div className="space-y-1 pl-5">
            <NamespaceSelect
              accountId={account.id}
              value={choice.namespace}
              onChange={(namespace) => onChange({ ...choice, namespace })}
              disabled={disabled}
            />
            <p className="text-[11px] text-muted-foreground">
              {t("forks.cloneHelp", { source: source.fullName })}
            </p>
          </div>
        )}
      </div>
    </>
  );
}

/** One line "Đang fork… / Đang chờ fork sẵn sàng…" while a fork flow runs. */
export function ForkPhaseLine({ phase }: { phase: ForkPhase | null }) {
  const { t } = useTranslation();
  if (!phase) return null;
  return (
    <p className="flex items-center gap-1.5 text-xs text-muted-foreground" aria-live="polite">
      <LoaderCircle className="size-3.5 animate-spin" />
      {t(`forks.phase.${phase}`)}
    </p>
  );
}
