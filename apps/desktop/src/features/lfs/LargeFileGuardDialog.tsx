import { useState } from "react";
import { LoaderCircle, OctagonX, TriangleAlert } from "lucide-react";
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
import { splitPath } from "@/features/commit/format";
import type { GuardFinding, GuardLevel, GuardReport } from "@/ipc";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import { choosePatterns, displaySize, relevantFindings, summarize, type TrackMode } from "./guard";

interface Props {
  /** Guard result to review; `null` = closed. */
  report: GuardReport | null;
  busy?: boolean;
  /** Track `patterns` with LFS, re-stage `paths` as pointers, then commit. */
  onTrack: (patterns: string[], paths: string[]) => void;
  onCommitAnyway: () => void;
  onCancel: () => void;
}

const LEVEL_CLASS: Record<GuardLevel, string> = {
  block: "border-danger/40 bg-danger/10 text-danger",
  warn: "border-warning/40 bg-warning/10 text-warning",
  suggest: "border-info/40 bg-info/10 text-info",
  none: "text-muted-foreground",
};

const LEVEL_KEY = {
  block: "guard.levelBlock",
  warn: "guard.levelWarn",
  suggest: "guard.levelSuggest",
} as const;

function Badge({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <span className={cn("shrink-0 rounded border px-1 text-[10px] leading-4", className)}>
      {children}
    </span>
  );
}

function FindingRow({
  finding: f,
  checked,
  onToggle,
}: {
  finding: GuardFinding;
  checked: boolean;
  onToggle: (checked: boolean) => void;
}) {
  const { t } = useTranslation();
  const { dir, name } = splitPath(f.path);
  const size = displaySize(f);
  return (
    <li className="flex items-start gap-2 px-2 py-1 text-xs">
      <input
        type="checkbox"
        checked={checked}
        // Over the hard limit: committing it as is cannot work, so it is always tracked.
        disabled={f.level === "block"}
        onChange={(e) => onToggle(e.target.checked)}
        aria-label={t("guard.select", { path: f.path })}
        className="mt-0.5 accent-primary"
      />
      <div className="min-w-0 flex-1">
        <div className="truncate font-mono" title={f.path}>
          <span className="text-muted-foreground">{dir}</span>
          {name}
        </div>
        <div className="mt-0.5 flex flex-wrap gap-1">
          {f.level !== "none" && (
            <Badge className={LEVEL_CLASS[f.level]}>{t(LEVEL_KEY[f.level])}</Badge>
          )}
          {f.looksBinary && <Badge>{t("guard.badgeBinary")}</Badge>}
          {f.looksLikeBuildOutput && <Badge>{t("guard.badgeBuild")}</Badge>}
          {f.alreadyLfs && <Badge>{t("guard.badgeLfs")}</Badge>}
          {f.needsRenormalize && <Badge>{t("guard.badgeRenormalize")}</Badge>}
        </div>
      </div>
      <span
        className={cn(
          "shrink-0 font-mono tabular-nums",
          f.level === "block" ? "text-danger" : f.level === "warn" ? "text-warning" : "",
        )}
      >
        {size === null ? "–" : formatBytes(size)}
      </span>
    </li>
  );
}

/** Large File Guard (PLAN §6.2): shown before a commit that would add big / binary files. */
export function LargeFileGuardDialog({ report, busy, onTrack, onCommitAnyway, onCancel }: Props) {
  const { t } = useTranslation();
  const findings = report ? relevantFindings(report) : [];
  const [mode, setMode] = useState<TrackMode>("extension");
  const [unchecked, setUnchecked] = useState<ReadonlySet<string>>(() => new Set());
  const selected = findings.filter((f) => !unchecked.has(f.path));
  const patterns = choosePatterns(selected, mode);
  const extPatterns = choosePatterns(findings, "extension").filter((p) => p.startsWith("*."));
  const summary = summarize(findings);

  const toggle = (path: string, checked: boolean) =>
    setUnchecked((prev) => {
      const next = new Set(prev);
      if (checked) next.delete(path);
      else next.add(path);
      return next;
    });

  const warnSize = report ? formatBytes(report.thresholds.warnBytes) : "";
  const blockBytes = report?.thresholds.blockBytes ?? null;

  return (
    <Dialog open={report !== null} onOpenChange={(o) => !o && !busy && onCancel()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <TriangleAlert className="size-5 shrink-0 text-warning" />
            {summary.over > 0
              ? t("guard.titleOver", { count: summary.over, size: warnSize })
              : t("guard.titleSuggest", { count: findings.length })}
          </DialogTitle>
          <DialogDescription>{t("guard.desc")}</DialogDescription>
        </DialogHeader>

        {summary.blocked && blockBytes !== null && (
          <p
            role="alert"
            className="flex items-start gap-2 rounded-md border border-danger/40 bg-danger/10 p-2 text-xs text-danger"
          >
            <OctagonX className="mt-0.5 size-3.5 shrink-0" />
            {t("guard.blockNote", { count: summary.block, size: formatBytes(blockBytes) })}
          </p>
        )}

        <ul className="max-h-[40vh] divide-y overflow-y-auto rounded-md border bg-background">
          {findings.map((f) => (
            <FindingRow
              key={f.path}
              finding={f}
              checked={!unchecked.has(f.path)}
              onToggle={(c) => toggle(f.path, c)}
            />
          ))}
        </ul>

        <div className="space-y-1.5 text-xs">
          <div
            role="radiogroup"
            aria-label={t("guard.mode")}
            className="flex flex-wrap gap-x-4 gap-y-1"
          >
            <span className="text-muted-foreground">{t("guard.mode")}</span>
            <label className="flex items-center gap-1.5">
              <input
                type="radio"
                name="guard-mode"
                checked={mode === "extension"}
                onChange={() => setMode("extension")}
                className="accent-primary"
              />
              {t("guard.byExtension")}
              {extPatterns.length > 0 && (
                <span className="font-mono text-muted-foreground">({extPatterns.join(", ")})</span>
              )}
            </label>
            <label className="flex items-center gap-1.5">
              <input
                type="radio"
                name="guard-mode"
                checked={mode === "path"}
                onChange={() => setMode("path")}
                className="accent-primary"
              />
              {t("guard.byPath")}
            </label>
          </div>
          <p className="break-all text-muted-foreground">
            {patterns.length > 0
              ? t("guard.willAdd", { patterns: patterns.join(", ") })
              : selected.length > 0
                ? t("guard.restageOnly")
                : " "}
          </p>
        </div>

        <DialogFooter className="sm:justify-between">
          <Button variant="outline" size="sm" disabled={busy} onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <div className="flex flex-col-reverse gap-2 sm:flex-row">
            <span title={t("guard.comingSoon")}>
              <Button variant="outline" size="sm" disabled className="w-full">
                {t("guard.gitignore")}
              </Button>
            </span>
            <span title={summary.blocked ? t("guard.commitAnywayBlocked") : undefined}>
              <Button
                variant="outline"
                size="sm"
                className="w-full"
                disabled={busy || summary.blocked}
                onClick={onCommitAnyway}
              >
                {t("guard.commitAnyway")}
              </Button>
            </span>
            <Button
              size="sm"
              autoFocus
              disabled={busy || selected.length === 0}
              onClick={() =>
                onTrack(
                  patterns,
                  selected.map((f) => f.path),
                )
              }
            >
              {busy && <LoaderCircle className="animate-spin" />}
              {t("guard.track")}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
