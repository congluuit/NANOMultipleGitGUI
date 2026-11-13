import { SearchCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { openTool } from "@/features/repo-tools/store";
import { sendsCodeAway, serviceName } from "../model";
import { useAiConfig } from "../queries";

/**
 * Next to the commit summary (only when AI is set up and on): opens the AI review of the
 * changes about to be committed. A click only opens the window; it asks before sending.
 */
export function AiReviewButton({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const config = useAiConfig();
  if (!config?.enabled) return null;
  const title = sendsCodeAway(config)
    ? `${t("aiReview.button")}\n${t("ai.sendsTo", { service: serviceName(config) })}`
    : t("aiReview.button");
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      className="shrink-0 text-primary"
      onClick={() => openTool({ kind: "aiReview", repoId })}
      title={title}
      aria-label={t("aiReview.button")}
      data-testid="ai-review"
    >
      <SearchCheck />
    </Button>
  );
}
