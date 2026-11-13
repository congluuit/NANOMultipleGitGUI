import { useEffect, useRef, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { type AiReview, commands } from "@/ipc";
import { useSettings } from "@/stores/settings";
import { errorFix, isAiError, modelOf, newRequestId, sendsCodeAway, serviceName } from "../model";
import { useAiConfig } from "../queries";
import { openAiSettings } from "../store";
import { aiErrorText } from "../suggest";
import { reviewConsentRemembered, rememberReviewConsent } from "./consent";

type Phase =
  | { step: "consent" }
  | { step: "running"; requestId: string }
  | { step: "done"; review: AiReview }
  | { step: "error"; error: unknown; unstaged: boolean };

/**
 * "AI review" of the changes about to be committed. Nothing is sent until the user agrees
 * here (or ticked "Don't ask again" before); it never runs by itself.
 */
export function AiReviewDialog({ repoId, onClose }: { repoId: number; onClose: () => void }) {
  const { t } = useTranslation();
  const config = useAiConfig();
  const [phase, setPhase] = useState<Phase>({ step: "consent" });
  const [remember, setRemember] = useState(false);
  const running = useRef<string | null>(null);
  const started = useRef(false);

  const run = async (unstaged: boolean) => {
    const requestId = newRequestId("review");
    running.current = requestId;
    setPhase({ step: "running", requestId });
    try {
      const review = await commands.aiReview(
        repoId,
        requestId,
        useSettings.getState().language,
        unstaged,
      );
      if (running.current === requestId) setPhase({ step: "done", review });
    } catch (error) {
      if (running.current !== requestId) return;
      if (isAiError(error) && error.kind === "cancelled") onClose();
      else setPhase({ step: "error", error, unstaged });
    } finally {
      if (running.current === requestId) running.current = null;
    }
  };

  // The user already said "Don't ask again": the click on the button was the go-ahead.
  useEffect(() => {
    if (started.current || !reviewConsentRemembered()) return;
    started.current = true;
    void run(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Closing the window stops a request that is still running. Deferred, so the effect
  // re-run of StrictMode / Fast Refresh does not cancel the request it just started.
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      window.setTimeout(() => {
        if (alive.current) return;
        const id = running.current;
        running.current = null;
        if (id) void commands.aiCancel(id).catch(() => false);
      }, 0);
    };
  }, []);

  if (!config) return null;
  const service = serviceName(config);
  const away = sendsCodeAway(config);
  const error = phase.step === "error" ? phase.error : null;
  const fix = isAiError(error) ? errorFix(error) : null;

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("aiReview.title")}</DialogTitle>
          <DialogDescription>
            {phase.step === "done"
              ? t("aiReview.doneDescription", {
                  count: phase.review.files,
                  service,
                  model: phase.review.model,
                })
              : t(away ? "aiReview.consent.cloud" : "aiReview.consent.local", {
                  service,
                  model: modelOf(config),
                })}
          </DialogDescription>
        </DialogHeader>

        {phase.step === "consent" && (
          <div className="grid gap-2 text-xs">
            <p>{t("aiReview.consent.what")}</p>
            <p className="text-muted-foreground">{t("aiReview.consent.never")}</p>
            <label className="flex cursor-pointer items-center gap-2">
              <input
                type="checkbox"
                checked={remember}
                onChange={(e) => setRemember(e.target.checked)}
              />
              {t("aiReview.consent.remember")}
            </label>
          </div>
        )}
        {phase.step === "running" && (
          <p className="flex items-center gap-2 text-xs text-muted-foreground" role="status">
            <LoaderCircle className="size-4 animate-spin" />
            {t("aiReview.running", { service })}
          </p>
        )}
        {phase.step === "done" && (
          <div className="grid gap-2">
            {phase.review.source === "unstaged" && (
              <p className="text-xs text-muted-foreground">{t("aiReview.unstagedUsed")}</p>
            )}
            <pre
              className="max-h-[55vh] overflow-auto rounded-md border bg-muted/40 p-3 font-sans text-xs leading-relaxed break-words whitespace-pre-wrap"
              data-testid="ai-review-text"
            >
              {phase.review.text}
            </pre>
            {phase.review.truncated && (
              <p className="text-[11px] text-warning">{t("aiReview.truncated")}</p>
            )}
            <p className="text-[11px] text-muted-foreground">{t("aiReview.disclaimer")}</p>
          </div>
        )}
        {phase.step === "error" && (
          <p className="text-xs break-words text-danger" role="alert">
            {aiErrorText(t, phase.error)}
          </p>
        )}

        <DialogFooter>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              const id = running.current;
              if (id) void commands.aiCancel(id).catch(() => false);
              onClose();
            }}
          >
            {phase.step === "done" ? t("common.close") : t("common.cancel")}
          </Button>
          {phase.step === "consent" && (
            <Button
              size="sm"
              autoFocus
              onClick={() => {
                rememberReviewConsent(remember);
                void run(false);
              }}
            >
              {t("aiReview.consent.confirm")}
            </Button>
          )}
          {phase.step === "done" && (
            <Button size="sm" variant="outline" onClick={() => void run(false)}>
              {t("aiReview.again")}
            </Button>
          )}
          {phase.step === "error" && fix === "settings" && (
            <Button
              size="sm"
              onClick={() => {
                onClose();
                openAiSettings();
              }}
            >
              {t("ai.openSettings")}
            </Button>
          )}
          {phase.step === "error" && fix === "unstaged" && (
            <Button size="sm" onClick={() => void run(true)}>
              {t("ai.useUnstaged")}
            </Button>
          )}
          {phase.step === "error" && fix === "retry" && (
            <Button size="sm" onClick={() => void run(phase.unstaged)}>
              {t("ai.retry")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
