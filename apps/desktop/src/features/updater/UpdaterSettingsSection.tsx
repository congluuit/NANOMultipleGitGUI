import { useState } from "react";
import { CheckCircle2, ExternalLink, Loader2, RefreshCw, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { openExternal } from "@/features/accounts/manager/openExternal";
import type { UpdateChannel } from "@/ipc";
import {
  checkForUpdates,
  clearSkippedVersion,
  setUpdateChannel,
  type CheckResult,
} from "./actions";
import { versionLabel } from "./notes";
import { useUpdaterStatus } from "./queries";
import { useUpdater } from "./store";

/** Settings → "Cập nhật": version, channel, manual check, last check. */
export function UpdaterSettingsSection() {
  const { t, i18n } = useTranslation();
  const status = useUpdaterStatus().data;
  const checking = useUpdater((s) => s.checking);
  const [result, setResult] = useState<CheckResult | null>(null);
  if (!status) return null;

  const releasesUrl = status.releasesUrl;
  const lastCheck =
    status.lastCheck !== null
      ? new Date(status.lastCheck * 1000).toLocaleString(i18n.language)
      : t("updater.settings.never");

  const outcome = (() => {
    if (!result) return null;
    if (result.error) {
      return (
        <p className="flex items-start gap-1.5 text-xs text-destructive" role="alert">
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
          <span>
            {t(`updater.error.${result.error.kind}`)} —{" "}
            {t(`updater.errorHelp.${result.error.kind}`)}
          </span>
        </p>
      );
    }
    if (result.check?.status === "upToDate") {
      return (
        <p className="flex items-center gap-1.5 text-xs text-muted-foreground" role="status">
          <CheckCircle2 className="size-3.5 text-primary" />
          {t("updater.upToDate", { version: versionLabel(result.check.currentVersion) })}
        </p>
      );
    }
    return null;
  })();

  return (
    <div className="space-y-2" data-testid="updater-settings">
      <div className="text-xs font-semibold">{t("updater.settings.title")}</div>
      <div className="text-xs text-muted-foreground">
        {t("updater.settings.current", { version: versionLabel(status.currentVersion) })}
      </div>

      {!status.configured ? (
        <div className="space-y-1.5 rounded-md border bg-muted/40 p-2.5 text-xs">
          <div className="font-medium">{t("updater.notConfigured")}</div>
          <div className="text-muted-foreground">
            {t(releasesUrl ? "updater.notConfiguredHint" : "updater.notConfiguredNoRepo")}
          </div>
          {releasesUrl && (
            <Button
              variant="outline"
              size="sm"
              className="h-7 text-xs"
              onClick={() => void openExternal(releasesUrl)}
            >
              <ExternalLink />
              {t("updater.action.openReleases")}
            </Button>
          )}
        </div>
      ) : (
        <>
          <div className="flex items-center gap-2">
            <Select
              value={status.channel}
              onValueChange={(v) => void setUpdateChannel(v as UpdateChannel)}
            >
              <SelectTrigger className="flex-1 text-xs" aria-label={t("updater.settings.channel")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="stable" className="text-xs">
                  {t("updater.channel.stable")}
                </SelectItem>
                <SelectItem value="beta" className="text-xs">
                  {t("updater.channel.beta")}
                </SelectItem>
              </SelectContent>
            </Select>
            <Button
              variant="outline"
              size="sm"
              disabled={checking}
              onClick={async () => {
                setResult(null);
                setResult(await checkForUpdates({ startup: false, notify: false }));
              }}
            >
              {checking ? <Loader2 className="animate-spin" /> : <RefreshCw />}
              {t("updater.cmd.check")}
            </Button>
          </div>
          <p className="text-[11px] text-muted-foreground">
            {t(`updater.channel.${status.channel}Hint`)}
          </p>
          {outcome}
          <div className="text-[11px] text-muted-foreground">
            {t("updater.settings.lastCheck", { date: lastCheck })}
          </div>
          {status.skippedVersion && (
            <div className="flex flex-wrap items-center gap-1 text-[11px] text-muted-foreground">
              {t("updater.settings.skipped", { version: versionLabel(status.skippedVersion) })}
              <Button
                variant="link"
                size="sm"
                className="h-auto px-0 text-[11px]"
                onClick={() => void clearSkippedVersion()}
              >
                {t("updater.settings.unskip")}
              </Button>
            </div>
          )}
        </>
      )}
    </div>
  );
}
