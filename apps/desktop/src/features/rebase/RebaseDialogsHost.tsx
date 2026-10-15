import { InteractiveRebaseDialog } from "./InteractiveRebaseDialog";
import { RebaseDialog } from "./RebaseDialog";
import { useRebaseDialog } from "./store";

/** Renders whichever rebase dialog was requested. Mounted once (in the toolbar). */
export function RebaseDialogsHost() {
  const dialog = useRebaseDialog((s) => s.dialog);
  const close = () => useRebaseDialog.getState().open(null);
  if (!dialog) return null;
  const key = JSON.stringify(dialog);
  return dialog.kind === "rebase" ? (
    <RebaseDialog key={key} dialog={dialog} onClose={close} />
  ) : (
    <InteractiveRebaseDialog key={key} dialog={dialog} onClose={close} />
  );
}
