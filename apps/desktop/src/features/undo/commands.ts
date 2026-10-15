/** Palette commands: Undo / Redo with the operation in the title. */
import { Redo2, Undo2 } from "lucide-react";
import { modKey, registerCommands, type PaletteCommand } from "@/features/command-palette/registry";
import { runUndo } from "./actions";
import { describeOp, isRunnable } from "./label";
import { useUndoStore } from "./store";

registerCommands("undo", (c): PaletteCommand[] => {
  const { t, repo } = c;
  if (!repo) return [];
  const repoId = repo.id;
  const state = useUndoStore.getState().byRepo[repoId];
  const undo = state?.undo ?? null;
  const redo = state?.redo ?? null;
  const group = t("cmd.group.history");
  const mod = modKey(c);
  return [
    {
      id: "undo.undo",
      title: undo ? t("undo.undoTip", { what: describeOp(t, undo.op) }) : t("undo.palette.undo"),
      group,
      icon: Undo2,
      keywords: ["undo", "hoan tac"],
      shortcut: `${mod}+Z`,
      when: () => isRunnable(undo),
      run: () => void runUndo(repoId, false),
    },
    {
      id: "undo.redo",
      title: redo ? t("undo.redoTip", { what: describeOp(t, redo.op) }) : t("undo.palette.redo"),
      group,
      icon: Redo2,
      keywords: ["redo", "lam lai"],
      shortcut: `${mod}+Shift+Z`,
      when: () => isRunnable(redo),
      run: () => void runUndo(repoId, true),
    },
  ];
});
