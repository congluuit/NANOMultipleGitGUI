/** Settings: turn the Secret Guard on or off, forget "always ignore" choices. */
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useUi } from "@/stores/ui";
import {
  DEFAULT_PREFS,
  clearIgnoredSecrets,
  setSecretGuardEnabled,
  useSecretGuardPrefs,
} from "./prefs";

export function SecretGuardSettingsSection() {
  const { t } = useTranslation();
  const prefs = useSecretGuardPrefs().data ?? DEFAULT_PREFS;

  const clear = async () => {
    if (await clearIgnoredSecrets()) {
      useUi.getState().showNotice({ kind: "info", message: t("secretGuard.settings.cleared") });
    }
  };

  return (
    <div className="space-y-2" data-testid="secret-guard-settings">
      <div className="flex items-start gap-3 rounded-md border p-3 text-xs">
        <label htmlFor="settings-secret-guard" className="grid flex-1 cursor-pointer gap-0.5">
          <span className="font-medium">{t("secretGuard.settings.title")}</span>
          <span className="text-[11px] text-muted-foreground">
            {t("secretGuard.settings.hint")}
          </span>
        </label>
        <Switch
          id="settings-secret-guard"
          checked={prefs.enabled}
          onCheckedChange={(on) => void setSecretGuardEnabled(on)}
          className="mt-0.5"
        />
      </div>
      {prefs.ignoredCount > 0 && (
        <Button variant="outline" size="sm" className="w-full" onClick={() => void clear()}>
          {t("secretGuard.settings.clear", { count: prefs.ignoredCount })}
        </Button>
      )}
    </div>
  );
}
