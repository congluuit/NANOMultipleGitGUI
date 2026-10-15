import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSetupStatus } from "@/features/onboarding/setup/queries";
import { useSetupDialog } from "@/features/onboarding/setup/store";
import { commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { useAccountDialogs } from "../dialogs";
import { noticeKeys } from "./model";
import { cliSyncKeys, useCliAccountsScan } from "./queries";

/**
 * Startup check (PLAN 3B.1): when gh / glab has accounts the app does not know yet and the user
 * has not dismissed exactly those, show a light notice (not a modal) with "Open account manager"
 * and "Skip". Silent for the whole session when the first-run setup opened itself (its accounts
 * step lists the same accounts) and while the setup or import wizard is on screen.
 * Waits until no other notice is visible so it never hides an error.
 */
function useCliSyncNotice() {
  const { t } = useTranslation();
  const status = useSetupStatus();
  // Latched: finishing a setup step clears `shouldAutoOpen` but this session stays quiet.
  const [firstRunSeen, setFirstRunSeen] = useState(false);
  if (status.data?.shouldAutoOpen && !firstRunSeen) setFirstRunSeen(true);
  const firstRun = status.data === undefined || firstRunSeen;
  const setupOpen = useSetupDialog((s) => s.open);
  const wizardOpen = useAccountDialogs((s) => s.importWizard) || setupOpen;
  const noticeFree = useUi((s) => s.notice === null);
  const scan = useCliAccountsScan(!firstRun);
  const shown = useRef(false);
  const data = scan.data;
  const ready = Boolean(data?.shouldNotify) && !firstRun && !wizardOpen && noticeFree;

  useEffect(() => {
    if (!ready || shown.current || !data) return;
    shown.current = true;
    const keys = noticeKeys(data);
    const ui = useUi.getState();
    ui.showNotice({
      kind: "info",
      message: t("cliSync.notice.message", { count: keys.length }),
      hint: t("cliSync.notice.hint"),
      actions: [
        {
          label: t("cliSync.notice.open"),
          run: () => {
            useUi.getState().showNotice(null);
            useAccountDialogs.getState().openManager();
          },
        },
        {
          label: t("cliSync.notice.dismiss"),
          run: () => {
            useUi.getState().showNotice(null);
            void commands
              .cliAccountsDismiss(keys)
              .then(() => queryClient.invalidateQueries({ queryKey: cliSyncKeys.scan }))
              .catch((e: unknown) => console.error("could not save cliSync.dismissed", e));
          },
        },
      ],
    });
  }, [ready, data, t]);
}

/** Mounted once in `App.tsx`. */
export function CliSyncNoticeHost() {
  useCliSyncNotice();
  return null;
}
