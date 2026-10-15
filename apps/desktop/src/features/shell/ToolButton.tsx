import { useEffect, useRef, type ReactNode } from "react";
import { ChevronDown, LoaderCircle, type LucideIcon } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { useToolbarOverflow } from "./toolbarOverflow";

/**
 * Icon + label button of the toolbar; `tooltip` defaults to the label.
 *
 * With an `id` it takes part in the toolbar overflow: when the row is too narrow it is hidden
 * (not duplicated) and the "⋯" menu runs the same handler with the same label / disabled
 * state. `menu` adds a ▾ dropdown next to it (shown as a submenu in the "⋯" menu).
 */
export function ToolButton({
  id,
  icon: Icon,
  label,
  tooltip,
  onClick,
  disabled,
  busy,
  menu,
  menuLabel,
  priority,
  badge,
}: {
  id?: string;
  icon: LucideIcon;
  label: string;
  tooltip?: string;
  onClick?: () => void;
  disabled?: boolean;
  busy?: boolean;
  menu?: ReactNode;
  menuLabel?: string;
  /** Overflow order: lower moves into the "⋯" menu first; "pin" never does. */
  priority?: number | "pin";
  /** Small count on the icon, e.g. "↑2" commits to push. */
  badge?: { text: string; tone: "success" | "info" | "warning" } | null;
}) {
  const tip = tooltip ?? label;
  const hidden = useToolbarOverflow((s) => !!id && s.hidden.includes(id));
  const latest = useRef({ onClick, menu });
  useEffect(() => {
    latest.current = { onClick, menu };
  });
  const hasMenu = menu !== undefined;
  useEffect(() => {
    if (!id) return;
    useToolbarOverflow.getState().register(id, {
      icon: Icon,
      label,
      tooltip: tip,
      disabled,
      busy,
      run: () => latest.current.onClick?.(),
      menuLabel: hasMenu ? menuLabel : undefined,
      renderMenu: hasMenu ? () => latest.current.menu : undefined,
    });
  }, [id, Icon, label, tip, disabled, busy, hasMenu, menuLabel]);
  useEffect(() => {
    if (!id) return;
    return () => useToolbarOverflow.getState().unregister(id);
  }, [id]);

  return (
    <span
      className={cn("inline-flex shrink-0 items-center", hidden && "hidden")}
      data-toolbar-item={id}
      data-toolbar-priority={priority}
    >
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
              className="flex h-10 min-w-12 flex-col items-center justify-center gap-0.5 rounded-md px-2 text-[11px] whitespace-nowrap text-muted-foreground hover:bg-surface-2 hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
            >
              <span className="relative">
                {busy ? (
                  <LoaderCircle className="size-4 animate-spin" />
                ) : (
                  <Icon className="size-4" />
                )}
                {badge && (
                  <span
                    data-testid={id ? `toolbar-badge-${id}` : undefined}
                    className={cn(
                      "absolute -top-1.5 left-2.5 rounded-full px-1 text-[9px] leading-[14px] font-bold whitespace-nowrap tabular-nums",
                      badge.tone === "success" && "bg-success text-white dark:text-background",
                      badge.tone === "info" && "bg-info text-white dark:text-background",
                      badge.tone === "warning" && "bg-warning text-white dark:text-background",
                    )}
                  >
                    {badge.text}
                  </span>
                )}
              </span>
              <span>{label}</span>
            </button>
          </span>
        </TooltipTrigger>
        <TooltipContent className="max-w-80 whitespace-pre-line">{tip}</TooltipContent>
      </Tooltip>
      {hasMenu && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label={menuLabel}
              className="-ml-1 flex h-10 w-4 items-center justify-center rounded-md text-muted-foreground hover:bg-surface-2 hover:text-foreground"
            >
              <ChevronDown className="size-3" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-64 text-xs">
            {menu}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </span>
  );
}
