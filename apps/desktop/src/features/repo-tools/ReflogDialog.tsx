import { useMemo, useState } from "react";
import { GitBranchPlus, History, LoaderCircle, LogIn, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { relativeTime } from "@/features/blame/blame";
import { goToCommit } from "@/features/command-palette/actions";
import { runOutcome } from "@/features/commits/outcome";
import { useRefDialog } from "@/features/commits/store";
import { commands, errorMessage, type ReflogEntry } from "@/ipc";
import { filterReflog } from "./model";
import { useReflog } from "./queries";

const short = (oid: string) => oid.slice(0, 7);

/** Where HEAD has been: the way back to a commit that no branch points to any more. */
export function ReflogDialog({ repoId, onClose }: { repoId: number; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const reflog = useReflog(repoId, true);
  const [query, setQuery] = useState("");
  const openRef = useRefDialog((s) => s.open);
  const rows = useMemo(() => filterReflog(reflog.data ?? [], query), [reflog.data, query]);
  // Read once when the window opens: relative times need not tick.
  const [now] = useState(() => Date.now() / 1000);

  const show = (e: ReflogEntry) => {
    goToCommit(repoId, e.oid);
    onClose();
  };
  const branchHere = (e: ReflogEntry) => {
    onClose();
    openRef({ kind: "createBranch", repoId, start: e.oid, label: short(e.oid) });
  };
  const checkout = (e: ReflogEntry) => {
    onClose();
    void runOutcome(
      repoId,
      () => commands.commitCheckout(repoId, e.oid),
      () => t("ops.checkedOutCommit", { oid: short(e.oid) }),
    );
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t("reflog.title")}</DialogTitle>
          <DialogDescription>{t("reflog.description")}</DialogDescription>
        </DialogHeader>
        <label className="flex items-center gap-2 rounded-md border bg-background px-2">
          <Search className="size-3.5 text-muted-foreground" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("reflog.filter")}
            aria-label={t("reflog.filter")}
            className="h-8 flex-1 bg-transparent text-xs outline-none"
          />
        </label>
        {reflog.isPending && (
          <p className="flex items-center gap-2 text-xs text-muted-foreground">
            <LoaderCircle className="size-4 animate-spin" />
            {t("tools.loading")}
          </p>
        )}
        {reflog.isError && <p className="text-xs text-danger">{errorMessage(reflog.error)}</p>}
        {reflog.data && (
          <ul className="max-h-[55vh] overflow-y-auto rounded-md border">
            {rows.map((e) => (
              <li
                key={e.index}
                className="group flex items-center gap-2 border-b px-2 py-1.5 text-xs last:border-b-0 hover:bg-accent"
              >
                <History className="size-3.5 shrink-0 text-muted-foreground" />
                <span className="w-16 shrink-0 font-mono text-[11px] text-muted-foreground">
                  {short(e.oid)}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate">
                    <span className="font-semibold">{e.action}</span>
                    {e.message && <span className="text-muted-foreground"> {e.message}</span>}
                  </span>
                  <span className="block truncate text-[11px] text-muted-foreground">
                    {e.subject}
                  </span>
                </span>
                <span className="shrink-0 text-[11px] text-muted-foreground group-focus-within:hidden group-hover:hidden">
                  {e.time > 0 ? relativeTime(e.time, now, i18n.language) : ""}
                </span>
                <span className="hidden shrink-0 gap-1 group-focus-within:flex group-hover:flex">
                  <Button size="xs" variant="outline" onClick={() => show(e)}>
                    {t("reflog.show")}
                  </Button>
                  <Button
                    size="xs"
                    variant="outline"
                    title={t("reflog.branchTip")}
                    onClick={() => branchHere(e)}
                  >
                    <GitBranchPlus />
                    {t("reflog.branch")}
                  </Button>
                  <Button
                    size="xs"
                    variant="outline"
                    title={t("menu.checkoutCommit")}
                    onClick={() => checkout(e)}
                  >
                    <LogIn />
                    {t("reflog.checkout")}
                  </Button>
                </span>
              </li>
            ))}
            {rows.length === 0 && (
              <li className="px-2 py-6 text-center text-xs text-muted-foreground">
                {t("reflog.empty")}
              </li>
            )}
          </ul>
        )}
      </DialogContent>
    </Dialog>
  );
}
