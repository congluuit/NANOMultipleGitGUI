import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, errorMessage, type AccountSettingsPatch } from "@/ipc";
import { refreshAccounts, useAccountSettings } from "./queries";
import { Callout } from "./ui";

/** `identity.writeToRepoConfig` / `ssh.writeCoreSshCommand`. */
export function SettingsTab() {
  const { t } = useTranslation();
  const settings = useAccountSettings();
  const [error, setError] = useState<string | null>(null);

  const update = async (patch: AccountSettingsPatch) => {
    setError(null);
    try {
      await commands.accountSettingsUpdate(patch);
      refreshAccounts();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const rows = [
    {
      key: "identityWriteToRepoConfig" as const,
      label: t("accounts.settings.writeIdentity"),
      hint: t("accounts.settings.writeIdentityHint"),
    },
    {
      key: "sshWriteCoreSshCommand" as const,
      label: t("accounts.settings.writeSsh"),
      hint: t("accounts.settings.writeSshHint"),
    },
  ];

  return (
    <div className="grid gap-3">
      {rows.map((row) => (
        <label key={row.key} className="flex items-start gap-3 rounded-md border p-3 text-xs">
          <input
            type="checkbox"
            className="mt-0.5 accent-primary"
            disabled={!settings.data}
            checked={settings.data?.[row.key] ?? false}
            onChange={(e) => void update({ [row.key]: e.target.checked })}
          />
          <span className="grid gap-0.5">
            <span className="font-medium">{row.label}</span>
            <span className="text-[11px] text-muted-foreground">{row.hint}</span>
          </span>
        </label>
      ))}
      {error && <Callout tone="danger">{error}</Callout>}
    </div>
  );
}
