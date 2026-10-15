import { Monitor, Moon, Palette, Sun, Users } from "lucide-react";
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
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { useAppInfo } from "@/features/app/useAppInfo";
import { SetupSettingsSection } from "@/features/onboarding/setup/SetupSettingsSection";
import { UpdaterSettingsSection } from "@/features/updater/UpdaterSettingsSection";
import { StorageSection } from "@/features/settings/storage/StorageSection";
import { LANGUAGES, type Language } from "@/i18n";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";
import type { ThemeMode } from "@/theme/theme";

export function SettingsDialog() {
  const { t } = useTranslation();
  const open = useUi((s) => s.settingsOpen);
  const setOpen = useUi((s) => s.setSettingsOpen);
  const setView = useUi((s) => s.setView);
  const theme = useSettings((s) => s.theme);
  const setTheme = useSettings((s) => s.setTheme);
  const language = useSettings((s) => s.language);
  const setLanguage = useSettings((s) => s.setLanguage);
  const appInfo = useAppInfo();

  const themes: { value: ThemeMode; label: string; icon: typeof Sun }[] = [
    { value: "light", label: t("settings.themeLight"), icon: Sun },
    { value: "dark", label: t("settings.themeDark"), icon: Moon },
    { value: "system", label: t("settings.themeSystem"), icon: Monitor },
  ];

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("settings.title")}</DialogTitle>
          <DialogDescription>{t("settings.description")}</DialogDescription>
        </DialogHeader>

        <div className="space-y-5 py-2">
          <div className="space-y-2">
            <div className="text-xs font-semibold">{t("settings.theme")}</div>
            <ToggleGroup
              type="single"
              variant="outline"
              value={theme}
              onValueChange={(v) => v && setTheme(v as ThemeMode)}
              className="w-full"
            >
              {themes.map(({ value, label, icon: Icon }) => (
                <ToggleGroupItem
                  key={value}
                  value={value}
                  aria-label={label}
                  className="flex-1 gap-1.5 text-xs data-[state=on]:border-primary data-[state=on]:text-primary"
                >
                  <Icon className="size-3.5" />
                  {label}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          </div>

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
