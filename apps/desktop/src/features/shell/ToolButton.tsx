import type { LucideIcon } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/** Icon + label button of the toolbar; `tooltip` defaults to the label. */
export function ToolButton({
  icon: Icon,
  label,
  tooltip,
  onClick,
  disabled,
}: {
  icon: LucideIcon;
  label: string;
  tooltip?: string;
  onClick?: () => void;
  disabled?: boolean;
}) {
  const tip = tooltip ?? label;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        {/* A disabled button gets no pointer events: the wrapper keeps the tooltip working. */}
        <span className="inline-flex" tabIndex={disabled ? 0 : undefined}>
          <button
            type="button"
            onClick={onClick}
            disabled={disabled}
            aria-label={label}
            aria-description={tip !== label ? tip : undefined}
            className="flex h-10 min-w-12 flex-col items-center justify-center gap-0.5 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-surface-2 hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
          >
            <Icon className="size-4" />
            <span>{label}</span>
          </button>
        </span>
      </TooltipTrigger>
      <TooltipContent className="max-w-80 whitespace-pre-line">{tip}</TooltipContent>
    </Tooltip>
  );
}
