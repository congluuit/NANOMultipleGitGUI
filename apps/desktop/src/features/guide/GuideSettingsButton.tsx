/** Phase 6.7: Settings → "Hướng dẫn đầy đủ" (right under "Hướng dẫn ban đầu"). */
import { BookOpen } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useUi } from "@/stores/ui";
import { openGuide } from "./store";

export function GuideSettingsButton() {
  const { t } = useTranslation();
  return (
    <div className="space-y-1.5">
      <Button
        variant="outline"
        size="sm"
        className="w-full"
        onClick={() => {
          useUi.getState().setSettingsOpen(false);
          openGuide();
        }}
      >
        <BookOpen />
        {t("guide.open")}
      </Button>
      <p className="text-[11px] text-muted-foreground">{t("guide.openHint")}</p>
    </div>
  );
}
