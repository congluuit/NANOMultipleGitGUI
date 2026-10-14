import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { accountLabel, cachedAccounts, settleOneShot } from "@/features/accounts/switcher";
import { repoAccount } from "@/features/transfers/actions";
import { formatBytes } from "@/features/transfers/format";
import { confirmLfsPush, push, usePaletteConfirm } from "./actions";

/** Force-push and LFS-upload confirmations for pushes started from the palette. */
export function PaletteConfirmHost() {
  const { t } = useTranslation();
  const confirm = usePaletteConfirm((s) => s.confirm);
  const close = () => usePaletteConfirm.getState().set(null);

  if (confirm?.kind === "force") {
    const { repoId, plan, refs } = confirm;
    return (
      <ConfirmDialog
        open
        title={t("remote.forceTitle", { branch: plan.branch, remote: plan.remote })}
        description={t("remote.forceBody")}
        confirmLabel={t("remote.forcePushConfirm")}
        destructive
        onConfirm={() => {
          close();
          void push(repoId, refs, true);
        }}
        onCancel={close}
      />
    );
  }

  if (confirm?.kind === "lfs") {
    const { repoId, plan, estimate, shot } = confirm;
    // Who the upload runs as: the one-shot override, else the repo's account.
    const shotAccount = shot
      ? (cachedAccounts().accounts.find((a) => a.id === shot.accountId) ?? null)
      : null;
    const account = shotAccount
      ? { label: accountLabel(shotAccount), color: shotAccount.color }
      : repoAccount(repoId);
    return (
      <ConfirmDialog
        open
        title={t("transfers.confirmTitle")}
        description={
          <span>
            {t("transfers.confirmBody", {
              count: estimate.objects.length,
              size: formatBytes(estimate.totalBytes),
            })}{" "}
            <span className="inline-flex items-center gap-1 font-medium text-foreground">
              <span
                className="inline-block size-2 rounded-full"
                style={{ background: account?.color ?? "var(--warning)" }}
              />
              {account?.label ?? t("transfers.noAccount")}
            </span>
          </span>
        }
        confirmLabel={t("transfers.confirmAction")}
        onConfirm={() => {
          close();
          void confirmLfsPush(repoId, plan, shot);
        }}
        onCancel={() => {
          settleOneShot(repoId, shot, false); // not pushed: keep it for the next push
          close();
        }}
      >
        <div className="space-y-1 text-xs text-muted-foreground">
          {estimate.unknownSizes > 0 && (
            <p>{t("transfers.unknownSizes", { count: estimate.unknownSizes })}</p>
          )}
          <ul className="max-h-32 overflow-y-auto font-mono text-[11px]">
            {estimate.objects.slice(0, 50).map((o) => (
              <li key={o.oid} className="flex gap-2">
                <span className="min-w-0 flex-1 truncate" title={o.path}>
                  {o.path}
                </span>
                <span className="shrink-0">{o.size ? formatBytes(o.size) : "?"}</span>
              </li>
            ))}
          </ul>
          <p>{t("transfers.confirmNote")}</p>
        </div>
      </ConfirmDialog>
    );
  }

  return null;
}
