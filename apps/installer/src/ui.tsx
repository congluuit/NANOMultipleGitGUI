import type { ButtonHTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

type Variant = "primary" | "outline" | "ghost";

const variants: Record<Variant, string> = {
  primary: "bg-primary text-primary-fg hover:bg-primary-hover shadow-sm",
  outline: "border bg-surface text-text hover:bg-surface-2",
  ghost: "text-muted hover:bg-surface-2 hover:text-text",
};

export function Button({
  variant = "primary",
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      type="button"
      className={cn(
        "inline-flex h-9 items-center justify-center gap-1.5 rounded-md px-4 text-[13px] font-medium transition-colors disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-4",
        variants[variant],
        className,
      )}
      {...props}
    />
  );
}

export function Checkbox({
  checked,
  onChange,
  children,
  disabled,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  children: ReactNode;
  disabled?: boolean;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-2.5 text-[13px] has-disabled:cursor-default has-disabled:opacity-60">
      <input
        type="checkbox"
        className="mt-0.5 size-4 shrink-0 accent-[var(--primary)]"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span>{children}</span>
    </label>
  );
}

type Tone = "info" | "warning" | "danger";

const tones: Record<Tone, string> = {
  info: "border-info/30 bg-info/8 text-text",
  warning: "border-warning/35 bg-warning/10 text-text",
  danger: "border-danger/35 bg-danger/8 text-text",
};

export function Notice({
  tone,
  icon,
  children,
}: {
  tone: Tone;
  icon: ReactNode;
  children: ReactNode;
}) {
  return (
    <div
      role={tone === "danger" ? "alert" : "status"}
      className={cn("flex gap-2.5 rounded-lg border px-3.5 py-2.5 text-[12.5px]", tones[tone])}
    >
      <span className="mt-px shrink-0 [&_svg]:size-4">{icon}</span>
      <div className="min-w-0">{children}</div>
    </div>
  );
}
