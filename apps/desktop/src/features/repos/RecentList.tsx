import { FolderGit2, TriangleAlert, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { forgetRepository, openRepository } from "./actions";
import { useRecentRepos } from "./queries";

export function RecentList({ limit = 12 }: { limit?: number }) {
  const { t } = useTranslation();
  const recent = useRecentRepos();
  const items = (recent.data ?? []).slice(0, limit);

  if (!items.length) {
    return <p className="px-1 text-xs text-muted-foreground">{t("repos.noRecent")}</p>;
  }
  return (
    <ul className="divide-y rounded-md border bg-surface">
      {items.map((r) => (
        <li key={r.id} className="group flex items-center gap-2 px-3 py-2">
          <button
            type="button"
            disabled={!r.exists}
            onClick={() => void openRepository(r.path)}
            className={cn(
              "flex min-w-0 flex-1 items-center gap-2 text-left",
              r.exists ? "hover:text-primary" : "cursor-not-allowed opacity-60",
            )}
            title={r.path}
          >
            {r.exists ? (
              <FolderGit2 className="size-4 shrink-0 text-muted-foreground" />
            ) : (
              <TriangleAlert className="size-4 shrink-0 text-warning" />
            )}
            <span className="min-w-0">
              <span className="block truncate text-sm font-medium">{r.name}</span>
              <span className="block truncate text-[11px] text-muted-foreground">
                {r.exists ? r.path : `${t("repos.missing")} · ${r.path}`}
              </span>
            </span>
          </button>
          <button
            type="button"
            aria-label={t("repos.remove")}
            title={t("repos.remove")}
            onClick={() => void forgetRepository(r.id)}
            className="rounded p-1 text-muted-foreground opacity-0 group-hover:opacity-100 hover:bg-surface-2 focus-visible:opacity-100"
          >
            <X className="size-3.5" />
          </button>
        </li>
      ))}
    </ul>
  );
}
