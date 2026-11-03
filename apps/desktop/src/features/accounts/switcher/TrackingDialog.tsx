import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import type { TrackingMismatch } from "./model";
import { trackingVars } from "./tracking";

/**
 * "Push to origin (github.com) as Pret?" before Push/Pull go through a remote of another
 * account than the repo's (toolbar and command palette): switch to the repo account's remote,
 * keep this one (remembered), or cancel.
 */
export function TrackingDialog({
  ask,
  onSwitch,
  onKeep,
  onCancel,
}: {
  ask: { op: "push" | "pull"; m: TrackingMismatch } | null;
  onSwitch: () => void;
  onKeep: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const vars = ask ? trackingVars(ask.m) : {};
  return (
    <ConfirmDialog
      open={ask !== null}
      title={ask ? t(`switcher.tracking.title.${ask.op}`, vars) : ""}
      description={ask ? t("switcher.tracking.body", vars) : ""}
      confirmLabel={ask ? t(`switcher.tracking.switch.${ask.op}`, vars) : ""}
      onConfirm={onSwitch}
      onCancel={onCancel}
    >
      {ask && (
        <div className="flex justify-end">
          <Button size="sm" variant="ghost" onClick={onKeep}>
            {t(`switcher.tracking.keep.${ask.op}`, vars)}
          </Button>
        </div>
      )}
    </ConfirmDialog>
  );
}
