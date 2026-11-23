import i18n from "i18next";
import { useRefDialog } from "@/features/commits/store";
import { commands, errorMessage, type MergeForecast } from "@/ipc";
import { useUi, type Notice } from "@/stores/ui";
import { LISTED_FILES } from "./queries";

/** Notice for "Check for conflicts with <current>" (no merge is started). */
export function forecastNotice(
  f: MergeForecast,
  rev: string,
  branch: string,
  openMerge?: () => void,
): Notice {
  const t = i18n.t.bind(i18n);
  const vars = { rev, branch };
  const merge = openMerge ? [{ label: t("mergeForecast.openMerge"), run: openMerge }] : [];
  switch (f.kind) {
    case "unknown":
      return { kind: "info", message: t("mergeForecast.notice.unknown") };
    case "upToDate":
      return { kind: "info", message: t("mergeForecast.notice.upToDate", vars) };
    case "fastForward":
      return {
        kind: "success",
        message: t("mergeForecast.notice.fastForward", vars),
        actions: merge,
      };
    case "clean":
      return { kind: "success", message: t("mergeForecast.notice.clean", vars), actions: merge };
    case "conflicts": {
      const count = f.conflicts.length;
      const listed = f.conflicts.slice(0, LISTED_FILES).join(", ");
      const rest = count - LISTED_FILES;
      // Info, not error: nothing failed, and error notices go through the recovery helpers
      // that read file names as git output.
      return {
        kind: "info",
        message: t("mergeForecast.notice.conflicts", { ...vars, count }),
        hint: rest > 0 ? `${listed} ${t("mergeForecast.more", { count: rest })}` : listed,
        detail: f.conflicts.join("\n"),
        actions: merge,
      };
    }
  }
}

/** Runs the forecast for merging `rev` into the current branch and shows the answer. */
export async function checkConflicts(repoId: number, rev: string, current: string) {
  const notify = useUi.getState().showNotice;
  try {
    const f = await commands.mergeForecast(repoId, rev, null);
    notify({
      ...forecastNotice(f, rev, current, () =>
        useRefDialog.getState().open({ kind: "merge", repoId, rev, into: current }),
      ),
      repoId,
    });
  } catch (e) {
    notify({ kind: "error", message: errorMessage(e), repoId });
  }
}
