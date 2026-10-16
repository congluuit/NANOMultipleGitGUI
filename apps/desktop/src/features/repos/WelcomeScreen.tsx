import {
  BookOpen,
  CircleCheck,
  Circle,
  Download,
  FolderOpen,
  FolderPlus,
  Loader2,
  RefreshCw,
  Settings,
  UserPlus,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { useAccounts } from "@/features/accounts/queries";
import { useAppInfo } from "@/features/app/useAppInfo";
import { openGuide } from "@/features/guide/store";
import { openCloneDialog } from "@/features/clone/commands";
import { checkForUpdates } from "@/features/updater/actions";
import { versionLabel } from "@/features/updater/notes";
import { useUpdater } from "@/features/updater/store";
import { useUi } from "@/stores/ui";
import { newRepositoryWithDialog, openRepositoryWithDialog } from "./actions";
import { checklistSteps, useChecklist, type ChecklistStep } from "./checklistStore";
import { useRecentRepos } from "./queries";
import { RecentList } from "./RecentList";

export function WelcomeScreen() {
  const { t } = useTranslation();
  return (
    <div className="flex min-h-0 flex-1 justify-center overflow-y-auto bg-background p-8">
      <div className="flex w-full max-w-xl flex-col gap-6 pt-8">
        <div className="flex items-center gap-4">
          <img src="/logo.png" alt="" width={56} height={56} />
          <div>
            <h1 className="text-xl font-bold">{t("repos.welcomeTitle")}</h1>
            <p className="text-sm text-muted-foreground">{t("repos.welcomeBody")}</p>
          </div>
        </div>
        <FirstRunChecklist />
        <div className="flex gap-2">
          <Button onClick={() => void openRepositoryWithDialog(t("repos.pickOpen"))}>
            <FolderOpen />
            {t("repos.open")}
          </Button>
          <Button
            variant="outline"
            onClick={() => void newRepositoryWithDialog(t("repos.pickNew"))}
          >
            <FolderPlus />
            {t("repos.new")}
          </Button>
          <Button variant="outline" onClick={() => openCloneDialog()}>
            <Download />
            {t("clone.open")}
          </Button>
        </div>
        <section className="space-y-2">
          <h2 className="text-xs font-semibold text-muted-foreground uppercase">
            {t("repos.recent")}
          </h2>
          <RecentList />
        </section>
        <WelcomeFooter />
      </div>
    </div>
  );
}

/**
 * Three steps for a first start: connect an account, open a repository, read the guide.
 * Ticks itself off from the real state, and hides once done or dismissed.
 */
function FirstRunChecklist() {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const recent = useRecentRepos();
  const dismissed = useChecklist((s) => s.dismissed);
  const guideSeen = useChecklist((s) => s.guideSeen);
  // Wait for the real counts: a flash of "not done" on every start would be noise.
  if (dismissed || !accounts.data || !recent.data) return null;
  const steps = checklistSteps({
    accounts: accounts.data.length,
    repos: recent.data.length,
    guideSeen,
  });
  const done = steps.filter((s) => s.done).length;
  if (done === steps.length) return null;

  const action = (step: ChecklistStep) => {
    switch (step.id) {
      case "account":
        return (
          <Button
            size="sm"
            variant="outline"
            onClick={() => useAccountDialogs.getState().openManager({ add: true })}
          >
            <UserPlus />
            {t("checklist.account.action")}
          </Button>
        );
      case "repo":
        return (
          <Button
            size="sm"
            variant="outline"
            onClick={() => void openRepositoryWithDialog(t("repos.pickOpen"))}
          >
            <FolderOpen />
            {t("repos.open")}
          </Button>
        );
      case "guide":
        return (
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              useChecklist.getState().markGuideSeen();
              openGuide();
            }}
          >
            <BookOpen />
            {t("checklist.guide.action")}
          </Button>
        );
    }
  };

  return (
    <section
      aria-label={t("checklist.title")}
      className="space-y-2 rounded-md border bg-surface p-3"
      data-testid="first-run-checklist"
    >
      <div className="flex items-center gap-2">
        <h2 className="flex-1 text-sm font-semibold">
          {t("checklist.title")}{" "}
          <span className="font-normal text-muted-foreground">
            {t("checklist.progress", { done, total: steps.length })}
          </span>
        </h2>
        <button
          type="button"
          aria-label={t("checklist.hide")}
          title={t("checklist.hide")}
          onClick={() => useChecklist.getState().dismiss()}
          className="rounded p-1 text-muted-foreground hover:bg-surface-2"
        >
          <X className="size-3.5" />
        </button>
      </div>
      <ol className="space-y-1.5">
        {steps.map((step) => (
          <li key={step.id} className="flex items-center gap-2 text-xs">
            {step.done ? (
              <CircleCheck className="size-4 shrink-0 text-success" />
            ) : (
              <Circle className="size-4 shrink-0 text-muted-foreground" />
            )}
            <span className="min-w-0 flex-1">
              <span className={step.done ? "text-muted-foreground line-through" : "font-medium"}>
                {t(`checklist.${step.id}.title`)}
              </span>
              {!step.done && (
                <span className="block text-[11px] text-muted-foreground">
                  {t(`checklist.${step.id}.hint`)}
                </span>
              )}
            </span>
            {!step.done && action(step)}
          </li>
        ))}
      </ol>
    </section>
  );
}

/**
 * Settings and "Check for updates" without a repository: the toolbar (and its buttons) only
 * exists once a tab is open, so a fresh install still has a way to update and configure.
 */
function WelcomeFooter() {
  const { t } = useTranslation();
  const appInfo = useAppInfo().data;
  const checking = useUpdater((s) => s.checking);
  const pending = useUpdater((s) => s.pending);
  const setSettingsOpen = useUi((s) => s.setSettingsOpen);
  return (
    <footer className="mt-auto flex flex-wrap items-center gap-2 border-t pt-4 text-xs text-muted-foreground">
      <span className="min-w-0 truncate">
        {appInfo ? `${appInfo.name} v${appInfo.version}` : t("common.backendBrowser")}
      </span>
      <span className="flex-1" />
      {pending ? (
        <Button size="sm" onClick={() => useUpdater.getState().reopen()}>
          <Download />
          {t("updater.pending.installVersion", { version: versionLabel(pending.update.version) })}
        </Button>
      ) : (
        <Button
          variant="ghost"
          size="sm"
          disabled={checking}
          onClick={() => void checkForUpdates({ startup: false })}
        >
          {checking ? <Loader2 className="animate-spin" /> : <RefreshCw />}
          {t("updater.cmd.check")}
        </Button>
      )}
      <Button variant="ghost" size="sm" onClick={() => setSettingsOpen(true)}>
        <Settings />
        {t("toolbar.settings")}
      </Button>
    </footer>
  );
}
