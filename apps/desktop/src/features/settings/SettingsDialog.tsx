import { Palette, Users } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { AiSettingsSection } from "@/features/ai/AiSettingsSection";
import { useAppInfo } from "@/features/app/useAppInfo";
import { BackupSettingsSection } from "@/features/backup/BackupSettingsSection";
import { BranchSwitchSettingsSection } from "@/features/branch-switch/BranchSwitchSettingsSection";
import { GuideSettingsButton } from "@/features/guide/GuideSettingsButton";
import { ThemeSettingsSection } from "@/features/custom-theme/ThemeSettingsSection";
import { SetupSettingsSection } from "@/features/onboarding/setup/SetupSettingsSection";
import { UpdaterSettingsSection } from "@/features/updater/UpdaterSettingsSection";
import { StorageSection } from "@/features/settings/storage/StorageSection";
import { LANGUAGES, type Language } from "@/i18n";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";

export function SettingsDialog() {
  const { t } = useTranslation();
  const open = useUi((s) => s.settingsOpen);
  const setOpen = useUi((s) => s.setSettingsOpen);
  const setView = useUi((s) => s.setView);
  const language = useSettings((s) => s.language);
  const setLanguage = useSettings((s) => s.setLanguage);
  const ciEnabled = useSettings((s) => s.ciEnabled);
  const setCiEnabled = useSettings((s) => s.setCiEnabled);
  const authorAvatars = useSettings((s) => s.authorAvatars);
  const setAuthorAvatars = useSettings((s) => s.setAuthorAvatars);
  const appInfo = useAppInfo();

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("settings.title")}</DialogTitle>
          <DialogDescription>{t("settings.description")}</DialogDescription>
        </DialogHeader>

        <div className="space-y-5 py-2">
          <ThemeSettingsSection />

          <div className="space-y-2">
            <div className="text-xs font-semibold">{t("settings.language")}</div>
            <Select value={language} onValueChange={(v) => setLanguage(v as Language)}>
              <SelectTrigger className="w-full text-xs" aria-label={t("settings.language")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {LANGUAGES.map((l) => (
                  <SelectItem key={l.code} value={l.code} className="text-xs">
                    {l.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <SettingSwitch
              id="settings-ci"
              label={t("settings.ciEnabled")}
              hint={t("settings.ciEnabledHint")}
              checked={ciEnabled}
              onChange={setCiEnabled}
            />
            <SettingSwitch
              id="settings-avatars"
              label={t("settings.authorAvatars")}
              hint={t("settings.authorAvatarsHint")}
              checked={authorAvatars}
              onChange={setAuthorAvatars}
            />
          </div>

          <Button
            variant="outline"
            size="sm"
            className="w-full"
            onClick={() => {
              setOpen(false);
              useAccountDialogs.getState().openManager();
            }}
          >
            <Users />
            {t("accounts.manager.open")}
          </Button>

          <Button
            variant="outline"
            size="sm"
            className="w-full"
            onClick={() => {
              setOpen(false);
              setView("palette");
            }}
          >
            <Palette />
            {t("settings.openPalette")}
          </Button>

          <SetupSettingsSection />
          <GuideSettingsButton />
          <BranchSwitchSettingsSection />
          <AiSettingsSection />
          <BackupSettingsSection />
          <UpdaterSettingsSection />
          <StorageSection />
        </div>

        <div className="text-[11px] text-muted-foreground">
          {t("settings.about")}:{" "}
          {appInfo.data
            ? `${appInfo.data.name} v${appInfo.data.version} · ${appInfo.data.os}/${appInfo.data.arch}`
            : t("common.backendBrowser")}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function SettingSwitch({
  id,
  label,
  hint,
  checked,
  onChange,
}: {
  id: string;
  label: string;
  hint: string;
  checked: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <div className="flex items-start gap-3 rounded-md border p-3 text-xs">
      <label htmlFor={id} className="grid flex-1 cursor-pointer gap-0.5">
        <span className="font-medium">{label}</span>
        <span className="text-[11px] text-muted-foreground">{hint}</span>
      </label>
      <Switch id={id} checked={checked} onCheckedChange={onChange} className="mt-0.5" />
    </div>
  );
}
