import { useEffect, useState } from "react";
import { CircleAlert, CircleCheck, Info, TriangleAlert, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useToolStatus } from "@/features/repos/queries";
import { useUi, type Notice } from "@/stores/ui";
import { noticeDuration } from "./noticeDuration";
import { toolProblems } from "./toolProblems";

function Bar({
  tone,
  children,
  onDismiss,
  aside,
  footer,
  ...rest
}: {
  tone: "danger" | "warning" | "info" | "success";
  children: React.ReactNode;
  onDismiss?: () => void;
  /** Shown just before the dismiss button (the countdown of a notice). */
  aside?: React.ReactNode;
  /** Pinned to the bottom edge (the countdown bar of a notice). */
  footer?: React.ReactNode;
} & Omit<React.HTMLAttributes<HTMLDivElement>, "children">) {
  const { t } = useTranslation();
  const Icon =
    tone === "danger"
      ? CircleAlert
      : tone === "warning"
        ? TriangleAlert
        : tone === "success"
          ? CircleCheck
          : Info;
  return (
    <div
      role={tone === "info" || tone === "success" ? "status" : "alert"}
      {...rest}
      className={cn(
        "relative flex items-center gap-2 border-b px-3 py-1.5 text-xs",
        tone === "danger" && "bg-danger/10 text-danger",
        tone === "warning" && "bg-warning/10 text-warning",
        tone === "info" && "bg-info/10 text-info",
        tone === "success" && "bg-success/10 text-success",
      )}
    >
      <Icon className="size-4 shrink-0" />
      <div className="flex min-w-0 flex-1 items-center gap-2">{children}</div>
      {aside}
      {onDismiss && (
        <button
          type="button"
          aria-label={t("common.dismiss")}
          onClick={onDismiss}
          className="rounded p-0.5 hover:bg-black/5 dark:hover:bg-white/10"
        >
          <X className="size-3.5" />
        </button>
      )}
      {footer}
    </div>
  );
}

export function ToolBanner() {
  const { t } = useTranslation();
  const tools = useToolStatus();
  const [lfsDismissed, setLfsDismissed] = useState(false);
  if (!tools.data) return null;
  const { blocking, warning } = toolProblems(tools.data);

  if (blocking) {
    return (
      <Bar tone="danger">
        <span className="flex-1">
          {blocking.issue === "missing"
            ? t("tools.gitMissing", { min: blocking.min })
            : t("tools.gitOld", { version: blocking.version, min: blocking.min })}
        </span>
        <Button size="xs" variant="outline" onClick={() => void tools.refetch()}>
          {t("tools.recheck")}
        </Button>
      </Bar>
    );
  }
  if (warning && !lfsDismissed) {
    return (
      <Bar tone="warning" onDismiss={() => setLfsDismissed(true)}>
        <span className="flex-1">
          {warning.issue === "missing"
            ? t("tools.lfsMissing", { min: warning.min })
            : t("tools.lfsOld", { version: warning.version, min: warning.min })}
        </span>
        <Button size="xs" variant="outline" onClick={() => void tools.refetch()}>
          {t("tools.recheck")}
        </Button>
      </Bar>
    );
  }
  return null;
}

const TICK_MS = 100;

/** A fresh countdown for every notice shown, even one with the same text. */
const noticeIds = new WeakMap<object, number>();
let nextNoticeId = 0;
function noticeId(notice: object): number {
  let id = noticeIds.get(notice);
  if (id === undefined) {
    id = ++nextNoticeId;
    noticeIds.set(notice, id);
  }
  return id;
}

export function NoticeBar() {
  const notice = useUi((s) => s.notice);
  if (!notice) return null;
  return <NoticeView key={noticeId(notice)} notice={notice} />;
}

function NoticeView({ notice }: { notice: Notice }) {
  const { t } = useTranslation();
  const showNotice = useUi((s) => s.showNotice);
  const total = noticeDuration(notice);
  const [left, setLeft] = useState(total);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const paused = hovered || focused;

  useEffect(() => {
    if (paused) return;
    const timer = window.setInterval(() => setLeft((l) => Math.max(0, l - TICK_MS)), TICK_MS);
    return () => window.clearInterval(timer);
  }, [paused]);

  useEffect(() => {
    if (left === 0 && useUi.getState().notice === notice) showNotice(null);
  }, [left, notice, showNotice]);

  const rich = Boolean(notice.hint || notice.detail || notice.actions?.length);
  const seconds = Math.ceil(left / 1000);
  return (
    <Bar
      tone={notice.kind === "error" ? "danger" : notice.kind === "success" ? "success" : "info"}
      onDismiss={() => showNotice(null)}
      onPointerEnter={() => setHovered(true)}
      onPointerLeave={() => setHovered(false)}
      onFocus={() => setFocused(true)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setFocused(false);
      }}
      aside={
        <span
          className="shrink-0 self-start pt-0.5 font-mono text-[11px] tabular-nums opacity-80"
          title={paused ? t("notice.paused") : t("notice.autoHide", { count: seconds })}
          aria-label={t("notice.autoHide", { count: seconds })}
        >
          {paused ? "❚❚" : `${seconds}s`}
        </span>
      }
      footer={
        <span
          aria-hidden
          className="absolute bottom-0 left-0 h-0.5 bg-current opacity-40 transition-[width] duration-100 ease-linear"
          style={{ width: `${(left / total) * 100}%` }}
        />
      }
    >
      {rich ? (
        <div className="grid min-w-0 flex-1 gap-1 py-0.5">
          <span className="font-medium break-words">{notice.message}</span>
          {notice.hint && (
            <span className="whitespace-pre-line text-foreground/90">{notice.hint}</span>
          )}
          {notice.actions?.length ? (
            <span className="flex flex-wrap gap-1.5">
              {notice.actions.map((a) => (
                <Button key={a.label} size="xs" variant="outline" onClick={a.run}>
                  {a.label}
                </Button>
              ))}
            </span>
          ) : null}
          {notice.detail && (
            <details className="text-muted-foreground">
              <summary className="cursor-pointer select-none">{t("notice.technical")}</summary>
              <pre className="mt-1 max-h-40 overflow-auto font-mono text-[11px] break-all whitespace-pre-wrap">
                {notice.detail}
              </pre>
            </details>
          )}
        </div>
      ) : (
        <span className="break-all">{notice.message}</span>
      )}
    </Bar>
  );
}
