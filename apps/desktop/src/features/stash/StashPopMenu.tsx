import { ArchiveRestore, ListChecks } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";
import { stashName } from "./actions";
import { stashMeta, stashTitle, type useStashToolbar } from "./useStashToolbar";

/** Newest entries shown in the ▾ menu; the picker lists them all. */
const MENU_LIMIT = 6;

/** ▾ menu of the toolbar Pop button: pop any recent stash directly, or open the picker. */
export function StashPopMenu({ stash }: { stash: ReturnType<typeof useStashToolbar> }) {
  const { t } = useTranslation();
  const recent = [...stash.stashes].sort((a, b) => a.index - b.index).slice(0, MENU_LIMIT);
  return (
    <>
      <DropdownMenuLabel>{t("stash.menuTitle", { count: stash.count })}</DropdownMenuLabel>
      {recent.map((s) => (
        <DropdownMenuItem
          key={s.index}
          disabled={!stash.canPop}
          onSelect={() => void stash.popIndex(s.index)}
          title={s.message}
        >
          <ArchiveRestore />
          <span className="grid min-w-0">
            <span className="truncate">
              <span className="mr-1.5 font-mono text-[10px] text-muted-foreground">
                {stashName(s.index)}
              </span>
              {stashTitle(s)}
            </span>
            {stashMeta(s) && (
              <span className="truncate text-[10px] text-muted-foreground">{stashMeta(s)}</span>
            )}
          </span>
        </DropdownMenuItem>
      ))}
      <DropdownMenuSeparator />
      <DropdownMenuItem onSelect={stash.openPicker}>
        <ListChecks />
        {t("stash.menuAll", { count: stash.count })}
      </DropdownMenuItem>
    </>
  );
}
