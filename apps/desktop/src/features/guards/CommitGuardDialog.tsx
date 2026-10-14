import { BadgeCheck, KeyRound, LoaderCircle, TriangleAlert, Unlink } from "lucide-react";
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
import type { AccountSuggestion, CommitFinding, CommitGuardReport } from "@/ipc";
import { orderedFindings, switchSuggestions } from "./model";

interface Props {
  /** `null` = closed. */
  report: CommitGuardReport | null;
  busy?: boolean;
  /** Label of the bound account (for "not this account's email"). */
  accountLabel?: string | null;
  onSwitch: (suggestion: AccountSuggestion) => void;
  onCommitAnyway: () => void;
  onCancel: () => void;
}

function Warning({
  icon,
  title,
  children,
}: {
  icon: React.ReactNode;
  title: string;
  children?: React.ReactNode;
}) {
  return (
    <div
      role="alert"
      className="flex items-start gap-2 rounded-md border border-warning/40 bg-warning/10 p-2 text-xs"
    >
      <span className="mt-0.5 shrink-0 text-warning">{icon}</span>
      <div className="min-w-0 space-y-0.5">
        <div className="font-medium text-warning">{title}</div>
        {children}
      </div>
    </div>
  );
}

function FindingBox({
  finding: f,
  report,
  accountLabel,
}: {
  finding: CommitFinding;
  report: CommitGuardReport;
  accountLabel: string | null;
}) {
  const { t } = useTranslation();
  switch (f.kind) {
    case "emailMismatch": {
      const rule = f.suggestions.find((s) => s.accountId === f.expectedAccountId);
      const label =
        f.reason === "ruleAccount"
          ? (rule?.label ?? f.expectedAccountId ?? "")
          : (accountLabel ?? report.accountId ?? "");
      return (
        <Warning
          icon={<TriangleAlert className="size-3.5" />}
          title={t("guards.commit.mismatchTitle", { email: f.email })}
        >
          <p>{t(`guards.commit.reason.${f.reason}`, { label })}</p>
          {f.expectedEmail && (
            <p className="text-muted-foreground">
              {t("guards.commit.expectedEmail", { email: f.expectedEmail })}
            </p>
          )}
        </Warning>
      );
    }
    case "unbound":
      return (
        <Warning icon={<Unlink className="size-3.5" />} title={t("guards.commit.unboundTitle")}>
          <p>{t("guards.commit.unboundDesc")}</p>
        </Warning>
      );
    case "emailNotVerified":
      return (
        <Warning
          icon={<BadgeCheck className="size-3.5" />}
          title={t("guards.commit.unverifiedTitle")}
        >
          <p>{t("guards.commit.unverified", { email: f.email, label: f.accountLabel })}</p>
        </Warning>
      );
    case "signingMissing":
      return (
        <Warning icon={<KeyRound className="size-3.5" />} title={t("guards.commit.signingTitle")}>
          <p>{t("guards.commit.signing")}</p>
        </Warning>
      );
  }
}

/** Commit Guard (PLAN §4.6): yellow warnings with [Đổi sang tài khoản X] / [Vẫn commit] / Hủy. */
export function CommitGuardDialog({
  report,
  busy,
  accountLabel = null,
  onSwitch,
  onCommitAnyway,
  onCancel,
}: Props) {
  const { t } = useTranslation();
  const findings = report ? orderedFindings(report) : [];
  const suggestions = report ? switchSuggestions(report) : [];
  const identity = report?.identity ?? null;

  return (
    <Dialog open={report !== null} onOpenChange={(o) => !o && !busy && onCancel()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <TriangleAlert className="size-5 shrink-0 text-warning" />
            {t("guards.commit.title")}
          </DialogTitle>
          <DialogDescription className="break-all">
            {identity
              ? t("guards.commit.identity", { name: identity.name, email: identity.email })
              : t("guards.commit.noIdentity")}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-2">
          {report &&
            findings.map((f, i) => (
              <FindingBox
                key={`${f.kind}-${i}`}
                finding={f}
                report={report}
                accountLabel={accountLabel}
              />
            ))}
        </div>

        <DialogFooter className="sm:justify-between">
          <Button variant="outline" size="sm" disabled={busy} onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <div className="flex flex-col-reverse gap-2 sm:flex-row sm:flex-wrap sm:justify-end">
            <Button variant="outline" size="sm" disabled={busy} onClick={onCommitAnyway}>
              {t("guards.commit.commitAnyway")}
            </Button>
            {suggestions.map((s, i) => (
              <Button
                key={s.accountId}
                size="sm"
                variant={i === 0 ? "default" : "secondary"}
                autoFocus={i === 0}
                disabled={busy}
                title={s.email ?? undefined}
                onClick={() => onSwitch(s)}
              >
                {busy && i === 0 ? (
                  <LoaderCircle className="animate-spin" />
                ) : (
                  <span
                    aria-hidden
                    className="size-2 shrink-0 rounded-full"
                    style={{ backgroundColor: s.color }}
                  />
                )}
                {t("guards.commit.switchTo", { label: s.label })}
              </Button>
            ))}
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
