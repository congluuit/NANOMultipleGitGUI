import { useEffect } from "react";
import { TooltipProvider } from "@/components/ui/tooltip";
import { CliSyncNoticeHost } from "@/features/accounts/cli-sync";
import { AccountManagerHost } from "@/features/accounts/manager";
import { AiSettingsHost } from "@/features/ai/AiSettingsDialog";
import { BackupHost } from "@/features/backup/BackupHost";
import { BranchSwitchDialogHost } from "@/features/branch-switch/BranchSwitchDialog";
import { GuideHost } from "@/features/guide/GuideDialog";
import { CloneDialogHost } from "@/features/clone/CloneDialog";
import { CommandPalette } from "@/features/command-palette";
import { ThemeManagerHost } from "@/features/custom-theme/ThemeManager";
import { ForkDialogHost } from "@/features/forks/ForkDialog";
import { PublishDialogHost } from "@/features/publish/PublishDialog";
import { GuardDialogsHost } from "@/features/guards";
import { ImportWizardHost } from "@/features/onboarding/ImportWizard";
import { IssueCreateHost } from "@/features/issues/IssueCreateDialog";
import { MyWorkHost } from "@/features/mywork/MyWorkPanel";
import { SetupHost } from "@/features/onboarding/setup/SetupWizard";
import { PalettePage } from "@/features/palette/PalettePage";
import { CreatePullDialogHost } from "@/features/pulls/CreatePullDialog";
import { InitRepoDialog } from "@/features/repos/InitRepoDialog";
// Plain-words explanations and one-click fixes for git errors in the notice bar.
import "@/features/recovery";
import { useRepoChangeEvents } from "@/features/repos/queries";
import { useTransferEvents } from "@/features/remote/transfers";
import { SettingsDialog } from "@/features/settings/SettingsDialog";
import { StorageHost } from "@/features/settings/storage/StorageHost";
import { AppShell } from "@/features/shell/AppShell";
import { ShortcutsHost } from "@/features/shortcuts/ShortcutsDialog";
import { SshKeysDialog } from "@/features/ssh/SshKeysDialog";
import { UpdaterHost } from "@/features/updater/UpdateDialog";
import i18n from "@/i18n";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";
import { useThemeSync } from "@/theme/useThemeSync";
import { useUiScale } from "@/features/settings/useUiScale";

export default function App() {
  useThemeSync();
  useUiScale();
  useRepoChangeEvents();
  useTransferEvents();
  const language = useSettings((s) => s.language);
  const view = useUi((s) => s.view);

  useEffect(() => {
    void i18n.changeLanguage(language);
    document.documentElement.lang = language;
  }, [language]);

  return (
    <TooltipProvider delayDuration={400}>
      {view === "palette" ? <PalettePage /> : <AppShell />}
      <SettingsDialog />
      <InitRepoDialog />
      <CommandPalette />
      <AccountManagerHost />
      <SshKeysDialog />
      <ImportWizardHost />
      <SetupHost />
      <CloneDialogHost />
      <ForkDialogHost />
      <PublishDialogHost />
      <GuardDialogsHost />
      <UpdaterHost />
      <CliSyncNoticeHost />
      <StorageHost />
      <IssueCreateHost />
      <MyWorkHost />
      <CreatePullDialogHost />
      <BackupHost />
      <ThemeManagerHost />
      {/* Phase 6.1 / 6.7 */}
      <BranchSwitchDialogHost />
      <GuideHost />
      <ShortcutsHost />
      {/* Phase 6.4 */}
      <AiSettingsHost />
    </TooltipProvider>
  );
}
