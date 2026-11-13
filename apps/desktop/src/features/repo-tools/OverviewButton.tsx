import { LayoutList } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ToolButton } from "@/features/shell/ToolButton";
import { useWorkspace } from "@/stores/workspace";
import { openTool } from "./store";

/** Toolbar button of "All repositories"; only with more than one repository open. */
export function OverviewButton() {
  const { t } = useTranslation();
  const many = useWorkspace((s) => s.tabs.length > 1);
  if (!many) return null;
  return (
    <ToolButton
      id="overview"
      priority={3}
      icon={LayoutList}
      label={t("overview.button")}
      tooltip={t("overview.tooltip")}
      onClick={() => openTool({ kind: "overview" })}
    />
  );
}
