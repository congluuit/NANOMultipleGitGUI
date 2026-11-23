import { isTauri } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import type { MergeForecast } from "@/ipc";
import { LISTED_FILES, useMergeForecast } from "./queries";

/**
 * One line (or a short list) under the merge / rebase options. `estimate` is for rebase, which
 * replays commits one by one and can conflict differently from a single merge. Up to date and
 * fast-forward are left to the rebase dialog's own summary.
 */
export function ForecastView({
  forecast,
  loading,
  estimate = false,
}: {
  forecast: MergeForecast | null | undefined;
  loading: boolean;
  estimate?: boolean;
}) {
  const { t } = useTranslation();
  if (loading) {
    return (
      <p role="status" className="text-xs text-muted-foreground">
        {t("mergeForecast.checking")}
      </p>
    );
  }
  if (!forecast) return null;
  const line = (text: string, className = "text-muted-foreground") => (
    <p role="status" className={`text-xs ${className}`}>
      {text}
    </p>
  );
  switch (forecast.kind) {
    case "unknown":
      return null;
    case "upToDate":
      return estimate ? null : line(t("mergeForecast.upToDate"));
    case "fastForward":
      return estimate ? null : line(t("mergeForecast.fastForward"), "text-success");
    case "clean":
      return line(
        estimate ? t("mergeForecast.rebaseClean") : t("mergeForecast.clean"),
        "text-success",
      );
    case "conflicts": {
      const files = forecast.conflicts;
      const count = files.length;
      return (
        <div
          role="status"
          className="space-y-1 rounded-md border border-warning/40 bg-warning/10 p-2 text-xs"
        >
          <p className="font-medium">
            {estimate
              ? t("mergeForecast.rebaseConflicts", { count })
              : t("mergeForecast.conflicts", { count })}
          </p>
          <ul className="space-y-0.5 font-mono">
            {files.slice(0, LISTED_FILES).map((f) => (
              <li key={f} className="truncate" title={f}>
                {f}
              </li>
            ))}
          </ul>
          {count > LISTED_FILES && (
            <p className="text-muted-foreground">
              {t("mergeForecast.more", { count: count - LISTED_FILES })}
            </p>
          )}
          {estimate && <p className="text-muted-foreground">{t("mergeForecast.rebaseNote")}</p>}
        </div>
      );
    }
  }
}

/** The forecast for a dialog: query plus view. */
export function MergeForecastLine({
  repoId,
  rev,
  base = null,
  estimate = false,
}: {
  repoId: number;
  rev: string;
  base?: string | null;
  estimate?: boolean;
}) {
  const q = useMergeForecast(repoId, rev, base);
  // A failed call (repository closed meanwhile) hides the line like an unknown forecast.
  return <ForecastView forecast={q.data} loading={q.isPending && isTauri()} estimate={estimate} />;
}
