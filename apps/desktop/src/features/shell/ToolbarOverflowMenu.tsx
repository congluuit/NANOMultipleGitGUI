import { Fragment } from "react";
import { LoaderCircle, MoreHorizontal } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { MORE_WIDTH, useToolbarOverflow } from "./toolbarOverflow";

/** "⋯" button holding the toolbar buttons that do not fit; absent while everything fits. */
export function ToolbarOverflowMenu() {
  const { t } = useTranslation();
  const hidden = useToolbarOverflow((s) => s.hidden);
  const items = useToolbarOverflow((s) => s.items);
  if (hidden.length === 0) return null;

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              data-toolbar-more=""
              aria-label={t("layout.moreActions")}
              style={{ width: MORE_WIDTH }}
              className="flex h-10 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-surface-2 hover:text-foreground data-[state=open]:bg-surface-2"
            >
              <MoreHorizontal className="size-4" />
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>{t("layout.moreActions")}</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="start" className="w-64 text-xs">
        {hidden.map((id) => {
          const item = items[id];
          if (!item) return null;
          const Icon = item.busy ? LoaderCircle : item.icon;
          return (
            <Fragment key={`overflow-${id}`}>
              <DropdownMenuItem
                disabled={item.disabled}
                onSelect={() => item.run()}
                title={item.tooltip !== item.label ? item.tooltip : undefined}
              >
                <Icon className={item.busy ? "animate-spin" : undefined} />
                {item.label}
              </DropdownMenuItem>
              {item.renderMenu && (
                <DropdownMenuSub>
                  <DropdownMenuSubTrigger className="pl-8">{item.menuLabel}</DropdownMenuSubTrigger>
                  <DropdownMenuSubContent className="w-64 text-xs">
                    {item.renderMenu()}
                  </DropdownMenuSubContent>
                </DropdownMenuSub>
              )}
            </Fragment>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
