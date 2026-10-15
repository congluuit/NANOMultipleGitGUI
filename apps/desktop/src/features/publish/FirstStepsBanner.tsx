import { useState } from "react";
import { Check, CloudUpload, PenLine, Rocket, Upload, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { focusCommitSummary, push } from "@/features/command-palette/actions";
import { useRepoTransfer } from "@/features/remote/transfers";
import { useRepoRefs } from "@/features/repos/queries";
import { cn } from "@/lib/utils";
import { firstSteps, showFirstSteps, type FirstStep } from "./model";
import { openPublishDialog } from "./store";

const dismissKey = (repoId: number) => `nanomgg.firstSteps.dismissed.${repoId}`;

function isDismissed(repoId: number): boolean {
  try {
    return localStorage.getItem(dismissKey(repoId)) === "1";
  } catch {
    return false;
  }
}

/**
 * Checklist under the toolbar for a project that is not on a server yet (a fresh `git init`):
 * ① first commit → ② publish to GitHub / GitLab → ③ push, with a button for the next step.
 * Hidden once everything is done, or for good with ✕ (per repository).
 */
export function FirstStepsBanner({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const refs = useRepoRefs(repoId).data;
  const running = useRepoTransfer(repoId);
  const [dismissed, setDismissed] = useState(() => isDismissed(repoId));
  if (!refs || dismissed || !showFirstSteps(refs)) return null;
  const { done, next, branch } = firstSteps(refs);
  if (!next) return null;
  const b = branch ?? "main";

  const dismiss = () => {
    try {
      localStorage.setItem(dismissKey(repoId), "1");
    } catch {
      // Not persisted: hidden for this session only.
    }
    setDismissed(true);
  };

  const action: Record<FirstStep, { icon: typeof Rocket; label: string; run: () => void }> = {
    commit: {
      icon: PenLine,
      label: t("firstSteps.action.commit"),
      run: () => void focusCommitSummary(repoId),
    },
    publish: {
      icon: CloudUpload,
      label: t("firstSteps.action.publish"),
      run: () => openPublishDialog(repoId),
    },
    push: {
      icon: Upload,
      label: t("firstSteps.action.push", { branch: b }),
      run: () => void push(repoId, refs, false),
    },
  };
  const a = action[next];

  return (
    <div
      role="region"
      aria-label={t("firstSteps.title")}
      className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b bg-primary/5 px-3 py-1.5 text-xs"
    >
      <Rocket className="size-4 shrink-0 text-primary" />
      <span className="font-semibold">{t("firstSteps.title")}</span>
      <ol className="flex flex-wrap items-center gap-x-3 gap-y-1">
        {(["commit", "publish", "push"] as const).map((s, i) => (
          <li
            key={s}
            className={cn(
              "flex items-center gap-1",
              done[s] && "text-muted-foreground line-through",
              s === next && "font-medium text-foreground",
            )}
          >
            <span
              className={cn(
                "inline-flex size-4 items-center justify-center rounded-full border text-[10px]",
                done[s] && "border-success bg-success text-white",
                s === next && "border-primary text-primary",
              )}
            >
              {done[s] ? <Check className="size-2.5" /> : i + 1}
            </span>
            {t(`firstSteps.step.${s}`, { branch: b })}
          </li>
        ))}
      </ol>
      <span className="text-muted-foreground">{t(`firstSteps.hint.${next}`, { branch: b })}</span>
      <div className="flex-1" />
      <Button
        size="xs"
        onClick={a.run}
        disabled={next === "push" && running !== null}
        data-testid="first-steps-action"
      >
        <a.icon />
        {a.label}
      </Button>
      <button
        type="button"
        aria-label={t("common.dismiss")}
        title={t("firstSteps.dismiss")}
        onClick={dismiss}
        className="rounded p-0.5 text-muted-foreground hover:bg-black/5 dark:hover:bg-white/10"
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}
