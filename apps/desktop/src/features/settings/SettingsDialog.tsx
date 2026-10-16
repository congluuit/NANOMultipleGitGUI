import { Heart, Palette, Users } from "lucide-react";
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
import { ShortcutsSettingsButton } from "@/features/shortcuts/ShortcutsDialog";
import { ThemeSettingsSection } from "@/features/custom-theme/ThemeSettingsSection";
import { SetupSettingsSection } from "@/features/onboarding/setup/SetupSettingsSection";
import { UpdaterSettingsSection } from "@/features/updater/UpdaterSettingsSection";
import { StorageSection } from "@/features/settings/storage/StorageSection";
import { LANGUAGES, type Language } from "@/i18n";
import { UI_SCALES, useSettings, type UiScale } from "@/stores/settings";
import { useUi } from "@/stores/ui";

const SCALE_LABELS = {
  1: "settings.scale.100",
  1.1: "settings.scale.110",
  1.2: "settings.scale.120",
  1.25: "settings.scale.125",
} as const satisfies Record<UiScale, string>;

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
  const uiScale = useSettings((s) => s.uiScale);
  const setUiScale = useSettings((s) => s.setUiScale);
  const confirmBranchSwitch = useSettings((s) => s.confirmBranchSwitch);
  const setConfirmBranchSwitch = useSettings((s) => s.setConfirmBranchSwitch);
  const appInfo = useAppInfo();

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle>{t("settings.title")}</DialogTitle>
          <DialogDescription>{t("settings.description")}</DialogDescription>
        </DialogHeader>

        <div className="grid gap-x-8 gap-y-5 py-2 md:grid-cols-2">
          <div className="min-w-0 space-y-5">
            <SectionTitle>{t("settings.general")}</SectionTitle>
            <ThemeSettingsSection />

            <div className="grid gap-4 sm:grid-cols-2">
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
                <div className="text-xs font-semibold">{t("settings.textSize")}</div>
                <Select
                  value={String(uiScale)}
                  onValueChange={(v) => setUiScale(Number(v) as UiScale)}
                >
                  <SelectTrigger className="w-full text-xs" aria-label={t("settings.textSize")}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {UI_SCALES.map((s) => (
                      <SelectItem key={s} value={String(s)} className="text-xs">
                        {t(SCALE_LABELS[s])}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <p className="text-[11px] text-muted-foreground sm:col-span-2">
                {t("settings.textSizeHint")}
              </p>
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
              <SettingSwitch
                id="settings-confirm-switch"
                label={t("settings.confirmBranchSwitch")}
                hint={t("settings.confirmBranchSwitchHint")}
                checked={confirmBranchSwitch}
                onChange={setConfirmBranchSwitch}
              />
            </div>

            <div className="grid gap-2 sm:grid-cols-2">
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
            </div>

            <SetupSettingsSection />
            <GuideSettingsButton />
            <ShortcutsSettingsButton />
            <BranchSwitchSettingsSection />
          </div>

          <div className="min-w-0 space-y-5">
            <SectionTitle>{t("settings.advanced")}</SectionTitle>
            <AiSettingsSection />
            <BackupSettingsSection />
            <UpdaterSettingsSection />
            <StorageSection />
          </div>
        </div>

        <DonateSection />

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

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <div className="border-b pb-1.5 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
      {children}
    </div>
  );
}

function DonateSection() {
  const { t } = useTranslation();
  return (
    <div
      className="flex flex-col items-center gap-3 rounded-md border bg-surface-2/40 p-4 text-center sm:flex-row sm:text-left"
      data-testid="settings-donate"
    >
      <img
        src="/donate-qr.jpg"
        alt="QR BIDV"
        className="size-40 shrink-0 rounded-md border bg-white object-contain"
      />
      <div className="grid gap-1">
        <span className="flex items-center justify-center gap-1.5 text-sm font-medium sm:justify-start">
          <Heart className="size-4 text-danger" />
          {t("settings.donate")}
        </span>
        <span className="text-xs text-muted-foreground">{t("settings.donateThanks")}</span>
      </div>
    </div>
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
