// Phase 6.2: Settings → theme: Light / Dark / System / Custom (+ theme list, "Manage themes…").
import { Monitor, Moon, Paintbrush, Sun, SwatchBook } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { errorMessage } from "@/ipc";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";
import type { ThemeMode } from "@/theme/theme";
import { PRESET_IDS } from "./model";
import { enableCustomMode, openThemeManager, useCustomThemes } from "./store";

export function ThemeSettingsSection() {
  const { t } = useTranslation();
  const theme = useSettings((s) => s.theme);
  const setTheme = useSettings((s) => s.setTheme);
  const themes = useCustomThemes((s) => s.themes);
  const activeId = useCustomThemes((s) => s.activeId);
  const activate = useCustomThemes((s) => s.activate);

  const modes: { value: ThemeMode; label: string; icon: typeof Sun }[] = [
    { value: "light", label: t("settings.themeLight"), icon: Sun },
    { value: "dark", label: t("settings.themeDark"), icon: Moon },
    { value: "system", label: t("settings.themeSystem"), icon: Monitor },
    { value: "custom", label: t("customTheme.mode"), icon: Paintbrush },
  ];

  const pick = (value: ThemeMode) => {
    if (value === "custom") void enableCustomMode();
    else setTheme(value);
  };

  return (
    <div className="space-y-2">
      <div className="text-xs font-semibold">{t("settings.theme")}</div>
      <ToggleGroup
        type="single"
        variant="outline"
        value={theme}
        onValueChange={(v) => v && pick(v as ThemeMode)}
        className="w-full"
      >
        {modes.map(({ value, label, icon: Icon }) => (
          <ToggleGroupItem
            key={`mode-${value}`}
            value={value}
            aria-label={label}
            className="flex-1 shrink gap-1 px-1.5 text-xs data-[state=on]:border-primary data-[state=on]:text-primary"
          >
            <Icon className="size-3.5" />
            <span className="truncate">{label}</span>
          </ToggleGroupItem>
        ))}
      </ToggleGroup>

      {theme === "custom" && (
        <div className="flex items-center gap-2">
          <Select
            value={activeId ?? undefined}
            onValueChange={(id) =>
              void activate(id).catch((e: unknown) =>
                useUi.getState().showNotice({
                  kind: "error",
                  message: t("customTheme.activateFailed"),
                  detail: errorMessage(e),
                }),
              )
            }
          >
            <SelectTrigger
              className="h-8 min-w-0 flex-1 text-xs"
              aria-label={t("customTheme.pickTheme")}
            >
              <SelectValue placeholder={t("customTheme.pickTheme")} />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                <SelectLabel className="text-[11px]">{t("customTheme.presets")}</SelectLabel>
                {PRESET_IDS.map((id) => (
                  <SelectItem key={`p-${id}`} value={id} className="text-xs">
                    {t(`customTheme.preset.${id}`)}
                  </SelectItem>
                ))}
              </SelectGroup>
              {themes.length > 0 && (
                <SelectGroup>
                  <SelectLabel className="text-[11px]">{t("customTheme.yourThemes")}</SelectLabel>
                  {themes.map((th) => (
                    <SelectItem key={`u-${th.id}`} value={th.id} className="text-xs">
                      {th.name}
                    </SelectItem>
                  ))}
                </SelectGroup>
              )}
            </SelectContent>
          </Select>
          <Button
            variant="outline"
            size="sm"
            className="h-8 text-xs"
            onClick={() => {
              useUi.getState().setSettingsOpen(false);
              openThemeManager(activeId);
            }}
          >
            <SwatchBook />
            {t("customTheme.manage")}
          </Button>
        </div>
      )}
    </div>
  );
}
