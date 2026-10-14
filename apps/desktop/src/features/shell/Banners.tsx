import { useState } from "react";
import { CircleAlert, Info, TriangleAlert, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useToolStatus } from "@/features/repos/queries";
import { useUi } from "@/stores/ui";
import { toolProblems } from "./toolProblems";

function Bar({
  tone,
  children,
  onDismiss,
}: {
  tone: "danger" | "warning" | "info";
  children: React.ReactNode;
  onDismiss?: () => void;
}) {
  const { t } = useTranslation();
  const Icon = tone === "danger" ? CircleAlert : tone === "warning" ? TriangleAlert : Info;
  return (
    <div
      role={tone === "info" ? "status" : "alert"}
      className={cn(
        "flex items-center gap-2 border-b px-3 py-1.5 text-xs",
        tone === "danger" && "bg-danger/10 text-danger",
        tone === "warning" && "bg-warning/10 text-warning",
        tone === "info" && "bg-info/10 text-info",
      )}
    >
      <Icon className="size-4 shrink-0" />
      <div className="flex min-w-0 flex-1 items-center gap-2">{children}</div>
      {onDismiss && (
        <button
          type="button"
          aria-label={t("common.dismiss")}
          onClick={onDismiss}
          className="rounded p-0.5 hover:bg-black/5 dark:hover:bg-white/10"
        >
          <X className="size-3.5" />
        </button>
      )}
    </div>
  );
}

export function ToolBanner() {
  const { t } = useTranslation();
  const tools = useToolStatus();
  const [lfsDismissed, setLfsDismissed] = useState(false);
  if (!tools.data) return null;
  const { blocking, warning } = toolProblems(tools.data);

  if (blocking) {
    return (
      <Bar tone="danger">
        <span className="flex-1">
          {blocking.issue === "missing"
            ? t("tools.gitMissing", { min: blocking.min })
            : t("tools.gitOld", { version: blocking.version, min: blocking.min })}
        </span>
        <Button size="xs" variant="outline" onClick={() => void tools.refetch()}>
          {t("tools.recheck")}
        </Button>
      </Bar>
    );
  }
  if (warning && !lfsDismissed) {
    return (
      <Bar tone="warning" onDismiss={() => setLfsDismissed(true)}>
        <span className="flex-1">
          {warning.issue === "missing"
            ? t("tools.lfsMissing", { min: warning.min })
            : t("tools.lfsOld", { version: warning.version, min: warning.min })}
        </span>
        <Button size="xs" variant="outline" onClick={() => void tools.refetch()}>
          {t("tools.recheck")}
        </Button>
      </Bar>
    );
  }
  return null;
}

export function NoticeBar() {
  const notice = useUi((s) => s.notice);
  const showNotice = useUi((s) => s.showNotice);
  if (!notice) return null;
  return (
    <Bar tone={notice.kind === "error" ? "danger" : "info"} onDismiss={() => showNotice(null)}>
      <span className="break-all">{notice.message}</span>
    </Bar>
  );
}
