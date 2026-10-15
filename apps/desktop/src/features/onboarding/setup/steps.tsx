import {
  Download,
  ExternalLink,
  FileInput,
  FolderOpen,
  Monitor,
  Moon,
  Sun,
  UserPlus,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { AccountAvatar } from "@/features/accounts/AccountAvatar";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { openExternal } from "@/features/accounts/manager/openExternal";
import { useAccounts, useProviders } from "@/features/accounts/queries";
import { useAppInfo } from "@/features/app/useAppInfo";
import { accountName } from "@/features/ssh/messages";
import { LANGUAGES, type Language } from "@/i18n";
import { useSettings } from "@/stores/settings";
import type { ThemeMode } from "@/theme/theme";
import { CliImportSlot } from "./CliImportSlot";
import { useSetupTools } from "./queries";

export function WelcomeStep() {
  const { t } = useTranslation();
  const theme = useSettings((s) => s.theme);
  const setTheme = useSettings((s) => s.setTheme);
  const language = useSettings((s) => s.language);
  const setLanguage = useSettings((s) => s.setLanguage);
  const info = useAppInfo().data;
  const repoUrl = info?.repositoryUrl ?? null;
  const themes: { value: ThemeMode; label: string; icon: typeof Sun }[] = [
    { value: "light", label: t("settings.themeLight"), icon: Sun },
    { value: "dark", label: t("settings.themeDark"), icon: Moon },
    { value: "system", label: t("settings.themeSystem"), icon: Monitor },
  ];
  return (
    <div className="space-y-5">
      <div className="flex items-center gap-4">
        <img src="/logo.png" alt="" width={56} height={56} className="shrink-0" />
        <div className="min-w-0 space-y-1">
          <h2 className="text-lg font-bold">{t("setup.welcome.title")}</h2>
          <p className="text-sm text-muted-foreground">{t("setup.welcome.tagline")}</p>
        </div>
      </div>
      <div className="space-y-2 text-sm">
        <p>{t("setup.welcome.about")}</p>
        {repoUrl && (
          <Button
            variant="link"
            size="sm"
            className="h-auto px-0"
            onClick={() => void openExternal(repoUrl)}
          >
            <ExternalLink />
            {t("setup.welcome.repo")}
          </Button>
        )}
      </div>
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
      </div>
    </div>
  );
}

export function AccountsStep() {
  const { t } = useTranslation();
  const accounts = useAccounts().data ?? [];
  const providers = useProviders().data ?? [];
  const dialogs = useAccountDialogs.getState;
  return (
    <div className="space-y-4">
      <p className="text-sm text-muted-foreground">{t("setup.accounts.intro")}</p>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" onClick={() => dialogs().openManager({ add: true })}>
          <UserPlus />
          {t("setup.accounts.signIn")}
        </Button>
        <Button size="sm" variant="outline" onClick={() => dialogs().openImportWizard()}>
          <FileInput />
          {t("setup.accounts.import")}
        </Button>
      </div>
      <CliImportSlot />
      <section className="space-y-2">
        <h3 className="text-xs font-semibold text-muted-foreground uppercase">
          {t("setup.accounts.list", { count: accounts.length })}
        </h3>
        {accounts.length === 0 ? (
          <p className="rounded-md border border-dashed px-3 py-3 text-xs text-muted-foreground">
            {t("setup.accounts.none")}
          </p>
        ) : (
          <ul className="space-y-1">
            {accounts.map((a) => (
              <li
                key={`account-${a.id}`}
                className="flex items-center gap-2 rounded-md border px-3 py-1.5 text-sm"
              >
                <AccountAvatar account={a} size={20} />
                <span className="min-w-0 flex-1 truncate font-medium">{accountName(a)}</span>
                <span className="truncate text-xs text-muted-foreground">
                  @{a.username} ·{" "}
                  {providers.find((p) => p.id === a.providerId)?.name ?? a.providerId}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>
      <p className="text-xs text-muted-foreground">{t("setup.accounts.later")}</p>
    </div>
  );
}

export function FinishStep({
  onOpenRepo,
  onClone,
}: {
  onOpenRepo: () => void;
  onClone: () => void;
}) {
  const { t } = useTranslation();
  const tools = useSetupTools(true).data;
  const accounts = useAccounts().data ?? [];
  const okTools = tools?.tools.filter((x) => x.state === "ok").length ?? 0;
  const missing = tools?.tools.filter((x) => x.state !== "ok" && x.importance !== "optional") ?? [];
  return (
    <div className="space-y-4">
      <p className="text-sm">{t("setup.finish.intro")}</p>
      <ul className="space-y-1 text-sm">
        {tools && (
          <li>
            {t("setup.finish.tools", { ok: okTools, total: tools.tools.length })}
            {missing.length > 0 &&
              ` — ${t("setup.finish.toolsMissing", {
                names: missing.map((x) => t(`setup.tool.${x.id}.name`)).join(", "),
              })}`}
          </li>
        )}
        <li>{t("setup.finish.accounts", { count: accounts.length })}</li>
      </ul>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" onClick={onOpenRepo}>
          <FolderOpen />
          {t("setup.finish.openRepo")}
        </Button>
        <Button size="sm" variant="outline" onClick={onClone}>
          <Download />
          {t("setup.finish.clone")}
        </Button>
      </div>
      <p className="text-xs text-muted-foreground">{t("setup.finish.rerunHint")}</p>
    </div>
  );
}
