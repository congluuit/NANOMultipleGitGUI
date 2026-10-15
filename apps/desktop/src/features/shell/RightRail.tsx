import { FilePen, GitCommitHorizontal, PanelRightOpen, type LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useLayout } from "@/stores/layout";

function RailButton({ icon: Icon, label }: { icon: LucideIcon; label: string }) {
  const toggle = useLayout((s) => s.togglePanel);
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          onClick={() => toggle("right")}
          className="flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <Icon className="size-4" />
        </button>
      </TooltipTrigger>
      <TooltipContent side="left">{label}</TooltipContent>
    </Tooltip>
  );
}

/** Collapsed right panel: expand it, or jump straight to what it shows. */
export function RightRail({ details }: { details: boolean }) {
  const { t } = useTranslation();
  return (
    <>
      <RailButton icon={PanelRightOpen} label={t("layout.expandRight")} />
      <span className="my-1 h-px w-6 bg-border" />
      {details ? (
        <RailButton icon={GitCommitHorizontal} label={t("details.title")} />
      ) : (
        <RailButton icon={FilePen} label={t("wip.title")} />
      )}
    </>
  );
}
