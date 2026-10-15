import { useTranslation } from "react-i18next";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { openCommandPalette } from "@/features/command-palette/store";
import type { IssueContextState } from "./useIssueContext";

/** Guidance (not an error) when issues cannot be loaded for a reason the user can fix. */
export function IssueContextHint({
  state,
  compact,
}: {
  state: Exclude<IssueContextState, { status: "ready" }>;
  compact?: boolean;
}) {
  const { t } = useTranslation();
  const openManager = useAccountDialogs((s) => s.openManager);
  const text =
    state.status === "loading"
      ? t("issues.loading")
      : state.status === "error"
        ? state.message
        : t(`issues.context.${state.status}`, {
            remote: "remote" in state ? state.remote.remote : "",
          });
  const action =
    state.status === "unbound"
      ? {
          label: t("issues.context.bind"),
          run: () => openCommandPalette({ commandId: "accounts.switch" }),
        }
      : state.status === "noToken"
        ? {
            label: t("issues.signInAgain"),
            run: () => openManager({ accountId: state.accountId, reauth: true }),
          }
        : null;
  return (
    <div className={compact ? "px-2 py-1 text-[11px]" : "p-4 text-xs"}>
      <p className="text-muted-foreground">{text}</p>
      {action && (
        <button
          type="button"
          onClick={action.run}
          className="mt-1 text-primary underline-offset-2 hover:underline"
        >
          {action.label}
        </button>
      )}
    </div>
  );
}
