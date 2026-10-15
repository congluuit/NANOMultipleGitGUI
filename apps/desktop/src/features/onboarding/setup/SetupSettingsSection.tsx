import { Compass } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useUi } from "@/stores/ui";
import { pendingSteps } from "./model";
import { useSetupStatus } from "./queries";
import { useSetupDialog } from "./store";

/** Settings → "Hướng dẫn ban đầu": re-run button, plus a gentle reminder of unfinished steps. */
export function SetupSettingsSection() {
  const { t } = useTranslation();
  const progress = useSetupStatus().data?.progress;
  const pending = progress ? pendingSteps(progress) : [];
  return (
    <div className="space-y-2">
      <div className="text-xs font-semibold">{t("setup.settings.title")}</div>
      {pending.length > 0 && (
        <p className="text-[11px] text-muted-foreground">
          {t("setup.settings.pending", {
            steps: pending.map((s) => t(`setup.step.${s}`)).join(", "),
          })}
        </p>
      )}
      <Button
        variant="outline"
        size="sm"
        className="w-full"
        onClick={() => {
          useUi.getState().setSettingsOpen(false);
          useSetupDialog.getState().openSetup(pending[0] ?? "welcome");
        }}
      >
        <Compass />
        {t("setup.settings.rerun")}
      </Button>
    </div>
  );
}
