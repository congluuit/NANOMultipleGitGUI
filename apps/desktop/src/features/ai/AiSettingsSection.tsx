import { Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useUi } from "@/stores/ui";
import { modelOf, serviceName } from "./model";
import { saveAiConfig, useAiConfig } from "./queries";
import { openAiSettings } from "./store";

/** Settings → "Gợi ý commit bằng AI": on/off, current service, entry to the AI settings. */
export function AiSettingsSection() {
  const { t } = useTranslation();
  const config = useAiConfig();
  const enabled = config?.enabled ?? false;

  return (
    <div className="space-y-2" data-testid="ai-settings">
      <div className="text-xs font-semibold">{t("ai.settings.title")}</div>
      <div className="flex items-start gap-3 rounded-md border p-3 text-xs">
        <label htmlFor="settings-ai" className="grid flex-1 cursor-pointer gap-0.5">
          <span className="font-medium">{t("ai.settings.enable")}</span>
          <span className="text-[11px] text-muted-foreground">
            {config && enabled
              ? t("ai.settings.using", { service: serviceName(config), model: modelOf(config) })
              : t("ai.settings.hint")}
          </span>
        </label>
        <Switch
          id="settings-ai"
          checked={enabled}
          disabled={!config}
          onCheckedChange={(on) => {
            if (!config) return;
            void saveAiConfig({ ...config, enabled: on });
            // First time on: show where to pick the service.
            if (on) {
              useUi.getState().setSettingsOpen(false);
              openAiSettings();
            }
          }}
          className="mt-0.5"
        />
      </div>
      <Button
        variant="outline"
        size="sm"
        className="w-full"
        onClick={() => {
          useUi.getState().setSettingsOpen(false);
          openAiSettings();
        }}
      >
        <Sparkles />
        {t("ai.settings.open")}
      </Button>
    </div>
  );
}
