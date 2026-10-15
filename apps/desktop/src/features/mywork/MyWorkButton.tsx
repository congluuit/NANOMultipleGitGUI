import { Inbox } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ToolButton } from "@/features/shell/ToolButton";
import { reviewCount } from "./model";
import { useMyWork } from "./queries";
import { openMyWork } from "./store";

/** Toolbar button of the "My work" panel; the label carries the "waiting for my review" count. */
export function MyWorkButton() {
  const { t } = useTranslation();
  const count = reviewCount(useMyWork().data);
  return (
    <ToolButton
      id="mywork"
      priority={4}
      icon={Inbox}
      label={
        count > 0
          ? t("mywork.buttonCount", { count: count > 99 ? "99+" : count })
          : t("mywork.button")
      }
      tooltip={count > 0 ? t("mywork.tooltipCount", { count }) : t("mywork.tooltip")}
      onClick={openMyWork}
    />
  );
}
