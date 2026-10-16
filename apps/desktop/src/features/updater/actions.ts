import { isTauri } from "@tauri-apps/api/core";
import i18n from "i18next";
import { openExternal } from "@/features/accounts/manager/openExternal";
import {
  commands,
  errorMessage,
  type UpdateChannel,
  type UpdateCheck,
  type UpdateError,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi, type Notice } from "@/stores/ui";
import { versionLabel } from "./notes";
import { updaterKeys } from "./queries";
import { useUpdater } from "./store";

const UPDATE_ERROR_KINDS = [
  "offline",
  "noRelease",
  "noBuildForPlatform",
  "badManifest",
  "badSignature",
  "downloadFailed",
  "notConfigured",
  "noPendingUpdate",
  "busy",
  "cancelled",
  "failed",
] as const satisfies readonly UpdateError["kind"][];

/** Anything thrown by an updater command, as an `UpdateError`. */
export function toUpdateError(value: unknown): UpdateError {
  if (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    (UPDATE_ERROR_KINDS as readonly unknown[]).includes((value as UpdateError).kind)
  ) {
    const e = value as UpdateError;
    return { kind: e.kind, message: typeof e.message === "string" ? e.message : "" };
  }
  return { kind: "failed", message: errorMessage(value) };
}

/** What went wrong and how to fix it, in plain words. */
export function updateErrorNotice(error: UpdateError, releasesUrl: string | null): Notice {
  const actions = releasesUrl
    ? [
        {
          label: i18n.t("updater.action.openReleases"),
          run: () => void openExternal(releasesUrl),
        },
      ]
    : [];
  return {
    kind: "error",
    message: i18n.t(`updater.error.${error.kind}`),
    hint: i18n.t(`updater.errorHelp.${error.kind}`),
    detail: error.message.trim() || undefined,
    actions,
  };
}

function notConfiguredNotice(check: UpdateCheck): Notice {
  const url = check.releasesUrl;
  return {
    kind: "info",
    message: i18n.t("updater.notConfigured"),
    hint: i18n.t(url ? "updater.notConfiguredHint" : "updater.notConfiguredNoRepo"),
    actions: url
      ? [{ label: i18n.t("updater.action.openReleases"), run: () => void openExternal(url) }]
      : [],
  };
}

export interface CheckResult {
  check: UpdateCheck | null;
  error: UpdateError | null;
}

/**
 * Checks GitHub Releases for a newer version.
 * - `startup`: once a day at most, silent unless there is an update the user did not skip.
 * - otherwise (menu / Settings): shows skipped versions too, and with `notify` (default for
 *   the menu) reports "up to date" / errors in the notice bar. Settings shows them inline.
 */
export async function checkForUpdates({
  startup,
  notify = !startup,
}: {
  startup: boolean;
  notify?: boolean;
}): Promise<CheckResult> {
  if (!isTauri()) return { check: null, error: null };
  if (!startup) useUpdater.getState().setChecking(true);
  try {
    const check = await commands.updaterCheck(startup);
    if (check.status === "available" && check.update) {
      if (!startup || !check.update.skipped) {
        useUpdater.getState().show(check.update, check.currentVersion);
      }
    } else if (check.status === "upToDate") {
      useUpdater.getState().clearPending();
      if (notify) {
        useUi.getState().showNotice({
          kind: "info",
          message: i18n.t("updater.upToDate", { version: versionLabel(check.currentVersion) }),
        });
      }
    } else if (notify && check.status === "notConfigured") {
      useUi.getState().showNotice(notConfiguredNotice(check));
    }
    return { check, error: null };
  } catch (e) {
    const error = toUpdateError(e);
    if (notify) {
      const status = queryClient.getQueryData<{ releasesUrl: string | null }>(updaterKeys.status);
      useUi.getState().showNotice(updateErrorNotice(error, status?.releasesUrl ?? null));
    } else if (startup) {
      console.warn("update check failed", error);
    }
    return { check: null, error };
  } finally {
    if (!startup) useUpdater.getState().setChecking(false);
    void queryClient.invalidateQueries({ queryKey: updaterKeys.status });
  }
}

/** Downloads + installs the update in the dialog; the app restarts when it succeeds. */
export async function installUpdate(): Promise<void> {
  const { update, installing } = useUpdater.getState();
  if (!update || installing) return;
  useUpdater.getState().startInstall();
  try {
    try {
      await commands.updaterInstall(update.version);
    } catch (e) {
      // The backend forgot the update (channel changed, another check): look it up again once.
      if (toUpdateError(e).kind !== "noPendingUpdate") throw e;
      const check = await commands.updaterCheck(false);
      if (check.update?.version !== update.version) throw e;
      await commands.updaterInstall(update.version);
    }
  } catch (e) {
    useUpdater.getState().failInstall(toUpdateError(e));
  }
}

/** "Bỏ qua bản này": no reminder at startup until a newer version appears. */
export async function skipUpdate(): Promise<void> {
  const { update } = useUpdater.getState();
  useUpdater.getState().close();
  useUpdater.getState().clearPending();
  if (!update || !isTauri()) return;
  try {
    const status = await commands.updaterSkipVersion(update.version);
    queryClient.setQueryData(updaterKeys.status, status);
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
  }
}

/** "Để sau": closes the dialog; the next startup check (tomorrow) asks again. */
export function postponeUpdate(): void {
  useUpdater.getState().close();
}

export async function setUpdateChannel(channel: UpdateChannel): Promise<void> {
  try {
    const status = await commands.updaterSetChannel(channel);
    queryClient.setQueryData(updaterKeys.status, status);
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
  }
}

export async function clearSkippedVersion(): Promise<void> {
  try {
    const status = await commands.updaterSkipVersion(null);
    queryClient.setQueryData(updaterKeys.status, status);
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
  }
}
