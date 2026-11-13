import { useState } from "react";
import { Bug, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { goToCommit } from "@/features/command-palette/actions";
import type { BisectMark, BisectState } from "@/ipc";
import { markBisect, startBisect, stopBisect } from "./actions";
import { bisectStepsLeft } from "./model";
import { useBisectState } from "./queries";

const short = (oid: string) => oid.slice(0, 7);

/** Asks for the broken and the last working commit, then starts the bisect. */
export function BisectStartDialog({
  repoId,
  good: initialGood,
  bad: initialBad,
  onClose,
}: {
  repoId: number;
  good: string | null;
  bad: string | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [bad, setBad] = useState(initialBad ?? "HEAD");
  const [good, setGood] = useState(initialGood ?? "");
  const [busy, setBusy] = useState(false);
  const ready = bad.trim() !== "" && good.trim() !== "";

  const start = async () => {
    setBusy(true);
    const ok = await startBisect(repoId, bad.trim(), good.trim());
    setBusy(false);
    if (ok) onClose();
  };
  const field =
    "h-8 w-full rounded-md border bg-background px-2 font-mono text-xs outline-none focus-visible:border-ring";

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("bisect.startTitle")}</DialogTitle>
          <DialogDescription>{t("bisect.startDescription")}</DialogDescription>
        </DialogHeader>
        <form
          className="grid gap-3 text-xs"
          onSubmit={(e) => {
            e.preventDefault();
            if (ready && !busy) void start();
          }}
        >
          <label className="grid gap-1">
            <span className="font-medium">{t("bisect.badLabel")}</span>
            <input
              value={bad}
              onChange={(e) => setBad(e.target.value)}
              spellCheck={false}
              className={field}
            />
            <span className="text-[11px] text-muted-foreground">{t("bisect.badHint")}</span>
          </label>
          <label className="grid gap-1">
            <span className="font-medium">{t("bisect.goodLabel")}</span>
            <input
              value={good}
              onChange={(e) => setGood(e.target.value)}
              spellCheck={false}
              autoFocus={!initialGood}
              placeholder={t("bisect.goodPlaceholder")}
              className={field}
            />
            <span className="text-[11px] text-muted-foreground">{t("bisect.goodHint")}</span>
          </label>
          <DialogFooter>
            <Button type="button" size="sm" variant="outline" onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={!ready || busy}>
              {busy && <LoaderCircle className="animate-spin" />}
              {t("bisect.start")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/** The banner for a known bisect state (tested on its own). */
export function BisectBannerView({ repoId, state: s }: { repoId: number; state: BisectState }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  if (!s.active) return null;

  const run = async (work: () => Promise<void>) => {
    setBusy(true);
    await work();
    setBusy(false);
  };
  const mark = (m: BisectMark) => () => void run(() => markBisect(repoId, m));
  const steps = bisectStepsLeft(s);

  return (
    <div
      role="status"
      className="flex shrink-0 items-center gap-2 border-b bg-info/10 px-3 py-1.5 text-xs"
    >
      <Bug className="size-4 shrink-0 text-info" />
      {s.firstBad ? (
        <>
          <span className="min-w-0 flex-1 truncate">
            <span className="font-semibold">{t("bisect.banner.found")}</span>{" "}
            <span className="font-mono">{short(s.firstBad)}</span> {s.firstBadSubject}
          </span>
          <Button
            variant="outline"
            size="xs"
            disabled={busy}
            onClick={() => s.firstBad && goToCommit(repoId, s.firstBad)}
          >
            {t("bisect.banner.show")}
          </Button>
          <Button size="xs" disabled={busy} onClick={() => void run(() => stopBisect(repoId))}>
            {t("bisect.banner.finish")}
          </Button>
        </>
      ) : (
        <>
          <span className="min-w-0 flex-1 truncate">
            <span className="font-semibold">{t("bisect.banner.title")}</span>{" "}
            {s.current && (
              <span title={s.currentSubject ?? undefined}>
                {t("bisect.banner.testing", { oid: short(s.current) })}{" "}
                <span className="text-muted-foreground">{s.currentSubject}</span>
              </span>
            )}
            <span className="ml-2 text-muted-foreground">
              {t("bisect.banner.left", { count: steps })}
            </span>
          </span>
          <Button
            size="xs"
            variant="outline"
            disabled={busy}
            title={t("bisect.banner.goodTip")}
            onClick={mark("good")}
          >
            {t("bisect.banner.good")}
          </Button>
          <Button
            size="xs"
            variant="outline"
            disabled={busy}
            title={t("bisect.banner.badTip")}
            onClick={mark("bad")}
          >
            {t("bisect.banner.bad")}
          </Button>
          <Button
            size="xs"
            variant="ghost"
            disabled={busy}
            title={t("bisect.banner.skipTip")}
            onClick={mark("skip")}
          >
            {t("bisect.banner.skip")}
          </Button>
          <Button
            size="xs"
            variant="ghost"
            disabled={busy}
            onClick={() => void run(() => stopBisect(repoId))}
          >
            {t("bisect.banner.stop")}
          </Button>
        </>
      )}
    </div>
  );
}

/** Shown while a bisect runs: says what to test and takes the answer. */
export function BisectBanner({ repoId }: { repoId: number }) {
  const state = useBisectState(repoId).data;
  if (!state?.active) return null;
  return <BisectBannerView repoId={repoId} state={state} />;
}
