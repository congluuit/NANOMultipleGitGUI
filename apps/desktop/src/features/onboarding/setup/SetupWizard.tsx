import { useEffect, useRef } from "react";
import { ArrowLeft, ArrowRight, Check, Compass } from "lucide-react";
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
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { openRepositoryWithDialog } from "@/features/repos/actions";
import type { SetupProgress, SetupStatus, SetupStep } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { cn } from "@/lib/utils";
import { STEPS, emptyProgress, withStepDone } from "./model";
import { saveProgress, setupKeys, useSetupStatus, useSetupTools } from "./queries";
import { AccountsStep, FinishStep, WelcomeStep } from "./steps";
import { useSetupDialog } from "./store";
import { ToolsStep } from "./ToolsStep";
import "./palette";

const currentProgress = (): SetupProgress =>
  queryClient.getQueryData<SetupStatus>(setupKeys.status)?.progress ?? emptyProgress();

/** Closing before the end (Bỏ qua, ×, Esc) remembers the skip; the steps done stay recorded. */
function leave() {
  useSetupDialog.getState().close();
  const progress = currentProgress();
  if (!progress.completed) void saveProgress({ ...progress, skipped: true });
}

/** Opens the setup once per session for a brand-new user (see `onboarding.rs`). */
function useAutoOpen() {
  const status = useSetupStatus();
  const opened = useRef(false);
  const should = status.data?.shouldAutoOpen ?? false;
  useEffect(() => {
    if (should && !opened.current) {
      opened.current = true;
      useSetupDialog.getState().openSetup("welcome");
    }
  }, [should]);
}

/** Mounted once in `App.tsx`. Hidden (state kept) while a dialog it opened is on screen. */
export function SetupHost() {
  useAutoOpen();
  const open = useSetupDialog((s) => s.open);
  const childOpen = useAccountDialogs(
    (s) => s.manager !== null || s.importWizard || s.clone !== null,
  );
  const visible = open && !childOpen;
  return (
    <Dialog open={visible} onOpenChange={(o) => !o && leave()}>
      {visible && <SetupWizard />}
    </Dialog>
  );
}

export function SetupWizard() {
  const { t } = useTranslation();
  const step = useSetupDialog((s) => s.step);
  const setStep = useSetupDialog((s) => s.setStep);
  const tools = useSetupTools(step === "tools" || step === "finish");
  const index = STEPS.indexOf(step);
  const blocked = step === "tools" && !tools.data?.canContinue;

  const go = (next: SetupStep) => {
    let progress = withStepDone(currentProgress(), step);
    if (next === "finish") {
      progress = { ...withStepDone(progress, "finish"), completed: true, skipped: false };
    }
    void saveProgress(progress);
    setStep(next);
  };
  const finishWith = (action?: () => void) => {
    useSetupDialog.getState().close();
    action?.();
  };

  return (
    <DialogContent className="flex max-h-[calc(100vh-2rem)] flex-col gap-3 sm:max-w-2xl">
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2">
          <Compass className="size-4" />
          {t("setup.title")}
        </DialogTitle>
        <DialogDescription>{t("setup.subtitle")}</DialogDescription>
      </DialogHeader>
      <ol className="flex flex-wrap gap-x-4 gap-y-1 text-xs" aria-label={t("setup.stepsLabel")}>
        {STEPS.map((s, i) => (
          <li
            key={`step-${s}`}
            aria-current={s === step ? "step" : undefined}
            className={cn(
              "flex items-center gap-1.5",
              s === step ? "font-semibold text-foreground" : "text-muted-foreground",
            )}
          >
            <span
              className={cn(
                "flex size-5 items-center justify-center rounded-full border text-[10px]",
                i < index && "border-primary bg-primary text-primary-foreground",
                s === step && "border-primary text-primary",
              )}
            >
              {i < index ? <Check className="size-3" /> : i + 1}
            </span>
            {t(`setup.step.${s}`)}
          </li>
        ))}
      </ol>
      <div className="min-h-0 flex-1 overflow-y-auto pr-1" data-testid="setup-body">
        {step === "welcome" && <WelcomeStep />}
        {step === "tools" && <ToolsStep />}
        {step === "accounts" && <AccountsStep />}
        {step === "finish" && (
          <FinishStep
            onOpenRepo={() => finishWith(() => void openRepositoryWithDialog(t("repos.pickOpen")))}
            onClone={() => finishWith(() => useAccountDialogs.getState().openClone())}
          />
        )}
      </div>
      <DialogFooter className="shrink-0 flex-row items-center gap-2 border-t pt-3 sm:justify-between">
        {step !== "finish" ? (
          <Button variant="ghost" size="sm" onClick={leave}>
            {t("setup.skip")}
          </Button>
        ) : (
          <span />
        )}
        <div className="flex items-center gap-2">
          {blocked && (
            <span className="hidden text-xs text-danger sm:inline">{t("setup.blocked")}</span>
          )}
          {index > 0 && step !== "finish" && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => setStep(STEPS[index - 1] ?? "welcome")}
            >
              <ArrowLeft />
              {t("setup.back")}
            </Button>
          )}
          {step === "finish" ? (
            <Button size="sm" onClick={() => finishWith()}>
              {t("setup.close")}
            </Button>
          ) : (
            <Button size="sm" disabled={blocked} onClick={() => go(STEPS[index + 1] ?? "finish")}>
              {t("setup.next")}
              <ArrowRight />
            </Button>
          )}
        </div>
      </DialogFooter>
    </DialogContent>
  );
}
