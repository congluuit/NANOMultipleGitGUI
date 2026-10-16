import { useEffect } from "react";
import { Keyboard } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useUi } from "@/stores/ui";
import { isCheatSheetKey, SHORTCUT_GROUPS, SHORTCUTS } from "./list";
import { openShortcuts, useShortcutsUi } from "./store";
import "./palette";

/** Mounted once in `App.tsx`: the `?` key and the cheat sheet dialog. */
export function ShortcutsHost() {
  const { t } = useTranslation();
  const open = useShortcutsUi((s) => s.open);
  const setOpen = useShortcutsUi((s) => s.setOpen);
  const isMac = navigator.userAgent.includes("Mac");

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.repeat || !isCheatSheetKey(e)) return;
      e.preventDefault();
      openShortcuts();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Keyboard className="size-4" />
            {t("shortcuts.title")}
          </DialogTitle>
          <DialogDescription>{t("shortcuts.description")}</DialogDescription>
        </DialogHeader>
        <div className="grid gap-x-8 gap-y-4 md:grid-cols-2">
          {SHORTCUT_GROUPS.map((group) => (
            <section key={group} className="space-y-1.5">
              <h3 className="border-b pb-1 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
                {t(`shortcuts.group.${group}`)}
              </h3>
              <dl className="space-y-1">
                {SHORTCUTS.filter((s) => s.group === group).map((s) => (
                  <div key={s.id} className="flex items-center justify-between gap-3 text-xs">
                    <dt>{t(`shortcuts.item.${s.id}`)}</dt>
                    <dd className="flex shrink-0 gap-1">
                      {s.keys(isMac).map((k) => (
                        <kbd
                          key={k}
                          className="rounded border bg-surface-2 px-1.5 py-0.5 font-mono text-[11px]"
                        >
                          {k === "Middle click" ? t("shortcuts.middleClick") : k}
                        </kbd>
                      ))}
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** Settings → "Keyboard shortcuts". */
export function ShortcutsSettingsButton() {
  const { t } = useTranslation();
  return (
    <Button
      variant="outline"
      size="sm"
      className="w-full"
      onClick={() => {
        useUi.getState().setSettingsOpen(false);
        openShortcuts();
      }}
    >
      <Keyboard />
      {t("shortcuts.title")}
    </Button>
  );
}
