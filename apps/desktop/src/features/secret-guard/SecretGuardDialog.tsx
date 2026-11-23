import { useState } from "react";
import { KeyRound, LoaderCircle } from "lucide-react";
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
import type { SecretFinding, SecretReport } from "@/ipc";
import {
  DEFAULT_CHOICE,
  SECRET_CHOICES,
  findingKey,
  planFor,
  sortFindings,
  type SecretChoice,
  type SecretPlan,
} from "./model";

interface Props {
  /** Scan result to review; `null` = closed. */
  report: SecretReport | null;
  busy?: boolean;
  /** Apply the per-finding choices (unstage, remember), then commit. */
  onApply: (plan: SecretPlan) => void;
  /** Ignore every finding this time and commit. */
  onCommitAnyway: () => void;
  onCancel: () => void;
}

const CHOICE_KEY = {
  unstage: "secretGuard.unstage",
  once: "secretGuard.once",
  always: "secretGuard.always",
} as const;

function FindingRow({
  finding: f,
  index,
  choice,
  onChoose,
}: {
  finding: SecretFinding;
  index: number;
  choice: SecretChoice;
  onChoose: (c: SecretChoice) => void;
}) {
  const { t } = useTranslation();
  const { dir, name } = splitPath(f.path);
  const where = `${f.path}:${f.line}`;
  return (
    <li className="space-y-1 px-2 py-1.5 text-xs" data-testid="secret-finding">
      <div className="flex items-start gap-2">
        <span className="shrink-0 rounded border border-danger/40 bg-danger/10 px-1 text-[10px] leading-4 text-danger">
          {f.ruleName}
        </span>
        <div className="min-w-0 flex-1 truncate font-mono" title={where}>
          <span className="text-muted-foreground">{dir}</span>
          {name}
          <span className="text-muted-foreground"> {t("secretGuard.line", { line: f.line })}</span>
        </div>
      </div>
      <div className="truncate font-mono text-[11px] text-muted-foreground" title={f.masked}>
        {f.masked}
      </div>
      <div
        role="radiogroup"
        aria-label={t("secretGuard.choiceFor", { path: f.path, line: f.line })}
        className="flex flex-wrap gap-x-4 gap-y-1"
      >
        {SECRET_CHOICES.map((c) => (
          <label key={c} className="flex items-center gap-1.5">
            <input
              type="radio"
              name={`secret-choice-${index}`}
              checked={choice === c}
              onChange={() => onChoose(c)}
              className="accent-primary"
            />
            {t(CHOICE_KEY[c])}
          </label>
        ))}
      </div>
    </li>
  );
}

/** Secret Guard: shown before a commit whose added lines look like credentials. */
export function SecretGuardDialog({ report, busy, onApply, onCommitAnyway, onCancel }: Props) {
  const { t } = useTranslation();
  const findings = report ? sortFindings(report.findings) : [];
  const [choices, setChoices] = useState<ReadonlyMap<string, SecretChoice>>(() => new Map());
  const choiceOf = (f: SecretFinding) => choices.get(findingKey(f)) ?? DEFAULT_CHOICE;
  const plan = planFor(findings, choices);

  const choose = (f: SecretFinding, c: SecretChoice) =>
    setChoices((prev) => new Map(prev).set(findingKey(f), c));
  const chooseAll = (c: SecretChoice) =>
    setChoices(new Map(findings.map((f) => [findingKey(f), c])));

  return (
    <Dialog open={report !== null} onOpenChange={(o) => !o && !busy && onCancel()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <KeyRound className="size-5 shrink-0 text-danger" />
            {t("secretGuard.title", { count: findings.length })}
          </DialogTitle>
          <DialogDescription>{t("secretGuard.desc")}</DialogDescription>
        </DialogHeader>

        <div className="flex flex-wrap items-center gap-2 text-xs">
          <span className="text-muted-foreground">{t("secretGuard.setAll")}</span>
          {SECRET_CHOICES.map((c) => (
            <Button
              key={c}
              variant="outline"
              size="sm"
              className="h-6 px-2 text-[11px]"
              disabled={busy}
              onClick={() => chooseAll(c)}
            >
              {t(CHOICE_KEY[c])}
            </Button>
          ))}
        </div>

        <ul className="max-h-[45vh] divide-y overflow-y-auto rounded-md border bg-background">
          {findings.map((f, i) => (
            <FindingRow
              key={findingKey(f)}
              finding={f}
              index={i}
              choice={choiceOf(f)}
              onChoose={(c) => choose(f, c)}
            />
          ))}
        </ul>

        <div className="space-y-1 text-[11px] text-muted-foreground">
          {plan.always.length > 0 && <p>{t("secretGuard.alwaysHint")}</p>}
          {report && report.ignored > 0 && (
            <p>{t("secretGuard.ignoredNote", { count: report.ignored })}</p>
          )}
          {report?.truncated && <p className="text-warning">{t("secretGuard.truncated")}</p>}
        </div>

        <DialogFooter className="sm:justify-between">
          <Button variant="outline" size="sm" disabled={busy} onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <div className="flex flex-col-reverse gap-2 sm:flex-row">
            <Button variant="outline" size="sm" disabled={busy} onClick={onCommitAnyway}>
              {t("secretGuard.commitAnyway")}
            </Button>
            <Button size="sm" autoFocus disabled={busy} onClick={() => onApply(plan)}>
              {busy && <LoaderCircle className="animate-spin" />}
              {t("secretGuard.apply")}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
