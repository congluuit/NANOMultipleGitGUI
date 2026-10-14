import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/utils";
import type { BadgeTone } from "./health";

export const inputClass =
  "h-8 w-full rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none aria-invalid:border-destructive disabled:opacity-60";

/** Labelled form field; `error` is shown under the control. */
export function Field({
  label,
  hint,
  error,
  children,
  className,
}: {
  label: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("grid gap-1 text-xs", className)}>
      <label className="grid gap-1">
        <span className="text-muted-foreground">{label}</span>
        {children}
      </label>
      {error ? (
        <span role="alert" className="text-[11px] text-destructive">
          {error}
        </span>
      ) : (
        hint && <span className="text-[11px] text-muted-foreground">{hint}</span>
      )}
    </div>
  );
}

export function TextInput({ className, ...props }: ComponentProps<"input">) {
  return <input {...props} className={cn(inputClass, className)} />;
}

const TONES: Record<BadgeTone, string> = {
  ok: "border-success/40 text-success",
  warn: "border-warning/50 text-warning",
  danger: "border-destructive/50 text-destructive",
  muted: "border-border text-muted-foreground",
};

export function Badge({
  tone = "muted",
  children,
  className,
}: {
  tone?: BadgeTone;
  children: ReactNode;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full border px-1.5 py-px text-[10px] leading-4 font-medium whitespace-nowrap",
        TONES[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}

/** Info / warning box. */
export function Callout({
  tone = "info",
  children,
  className,
}: {
  tone?: "info" | "warn" | "danger";
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      role={tone === "danger" ? "alert" : undefined}
      className={cn(
        "rounded-md border px-3 py-2 text-xs",
        tone === "info" && "border-border bg-muted/40",
        tone === "warn" && "border-warning/50 bg-warning/10",
        tone === "danger" && "border-destructive/50 bg-destructive/10 text-destructive",
        className,
      )}
    >
      {children}
    </div>
  );
}
