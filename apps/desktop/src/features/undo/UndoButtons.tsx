import { Redo2, Undo2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ToolButton } from "@/features/shell/ToolButton";
import type { UndoState } from "@/ipc";
import { runUndo } from "./actions";
import { describeOp, isRunnable, stepTooltip } from "./label";
import { useUndoState } from "./queries";
import { useUndoShortcuts } from "./shortcuts";
import { useUndoStore } from "./store";

/** Presentational pair (tested on its own). */
export function UndoRedoButtons({
  state,
  onUndo,
  onRedo,
}: {
  state: UndoState | undefined;
  onUndo: () => void;
  onRedo: () => void;
}) {
  const { t } = useTranslation();
  const undo = state?.undo ?? null;
  const redo = state?.redo ?? null;
  return (
    <>
      <ToolButton
        id="undo"
        icon={Undo2}
        label={t("toolbar.undo")}
        tooltip={stepTooltip(t, undo, false)}
        onClick={onUndo}
        disabled={!isRunnable(undo)}
      />
      <ToolButton
        id="redo"
        icon={Redo2}
        label={t("toolbar.redo")}
        tooltip={stepTooltip(t, redo, true)}
        onClick={onRedo}
        disabled={!isRunnable(redo)}
      />
    </>
  );
}

/** Toolbar Undo / Redo for the active repository, with shortcuts and the pushed-commit check. */
export function UndoButtons({ repoId }: { repoId: number | null }) {
  const { t } = useTranslation();
  const state = useUndoState(repoId).data;
  useUndoShortcuts(repoId);
  const confirm = useUndoStore((s) => s.confirm);
  const close = () => useUndoStore.getState().setConfirm(null);
  return (
    <>
      <UndoRedoButtons
        state={repoId === null ? undefined : state}
        onUndo={() => repoId !== null && void runUndo(repoId, false)}
        onRedo={() => repoId !== null && void runUndo(repoId, true)}
      />
      {confirm && (
        <ConfirmDialog
          open
          title={t("undo.confirmTitle")}
          description={t("undo.confirmBody", { what: describeOp(t, confirm.step.op) })}
          confirmLabel={t("undo.confirm")}
          destructive
          onConfirm={() => {
            close();
            void runUndo(confirm.repoId, false, true);
          }}
          onCancel={close}
        />
      )}
    </>
  );
}
