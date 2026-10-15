import { useEffect } from "react";
import { TooltipProvider } from "@/components/ui/tooltip";
import { CliSyncNoticeHost } from "@/features/accounts/cli-sync";
import { AccountManagerHost } from "@/features/accounts/manager";
import { CloneDialogHost } from "@/features/clone/CloneDialog";
import { CommandPalette } from "@/features/command-palette";
import { GuardDialogsHost } from "@/features/guards";
import { ImportWizardHost } from "@/features/onboarding/ImportWizard";
import { SetupHost } from "@/features/onboarding/setup/SetupWizard";
import { PalettePage } from "@/features/palette/PalettePage";
import { InitRepoDialog } from "@/features/repos/InitRepoDialog";
import { useRepoChangeEvents } from "@/features/repos/queries";
import { useTransferEvents } from "@/features/remote/transfers";
import { SettingsDialog } from "@/features/settings/SettingsDialog";
import { StorageHost } from "@/features/settings/storage/StorageHost";
import { AppShell } from "@/features/shell/AppShell";
import { SshKeysDialog } from "@/features/ssh/SshKeysDialog";
import { UpdaterHost } from "@/features/updater/UpdateDialog";
import i18n from "@/i18n";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";
import { useThemeSync } from "@/theme/useThemeSync";

export default function App() {
  useThemeSync();
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
      <GuardDialogsHost />
      <UpdaterHost />
      <CliSyncNoticeHost />
      <StorageHost />
    </TooltipProvider>
  );
}
