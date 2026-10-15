/** Phase 6.1: Settings → "Chuyển nhánh". */
import { useTranslation } from "react-i18next";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import type { DirtyAction } from "@/ipc";
import { DEFAULT_PREFS } from "./model";
import { savePrefs, useBranchSwitchPrefs } from "./prefs";

const ACTIONS: DirtyAction[] = ["ask", "stash", "carry"];

export function BranchSwitchSettingsSection() {
  const { t } = useTranslation();
  const prefs = useBranchSwitchPrefs().data ?? DEFAULT_PREFS;
  return (
    <div className="space-y-2" data-testid="branch-switch-settings">
      <div className="text-xs font-semibold">{t("branchSwitch.settings.title")}</div>
      <p className="text-[11px] text-muted-foreground">{t("branchSwitch.settings.intro")}</p>
      <label className="grid gap-1.5 text-xs">
        <span>{t("branchSwitch.settings.dirtyAction")}</span>
        <Select
          value={prefs.dirtyAction}
          onValueChange={(v) => void savePrefs({ dirtyAction: v as DirtyAction })}
        >
          <SelectTrigger
            className="w-full text-xs"
            aria-label={t("branchSwitch.settings.dirtyAction")}
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {ACTIONS.map((a) => (
              <SelectItem key={a} value={a} className="text-xs">
                {t(`branchSwitch.settings.action.${a}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </label>
      <div className="flex items-start gap-3 rounded-md border p-3 text-xs">
        <label
          htmlFor="settings-branch-auto-restore"
          className="grid flex-1 cursor-pointer gap-0.5"
        >
          <span className="font-medium">{t("branchSwitch.settings.autoRestore")}</span>
          <span className="text-[11px] text-muted-foreground">
            {t("branchSwitch.settings.autoRestoreHint")}
          </span>
        </label>
        <Switch
          id="settings-branch-auto-restore"
          checked={prefs.autoRestore}
          onCheckedChange={(on) => void savePrefs({ autoRestore: on })}
          className="mt-0.5"
        />
      </div>
    </div>
  );
}
