import { LogIn, Plus, UserRoundX, Wand2, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { AccountAvatar } from "../AccountAvatar";
import { useAccountDialogs } from "../dialogs";
import { useProviders } from "../queries";
import { bindRepo } from "./actions";
import { dismissBanner, useBannerDismissed } from "./bannerStore";
import { useRepoAccount, useSortedAccounts } from "./hooks";
import { accountLabel, missingSignIn, signInName } from "./model";

/** At most this many quick "bind to …" buttons. */
const QUICK = 5;

/**
 * "Repo này chưa gắn tài khoản — chọn tài khoản" (PLAN §4.3 step 5) with one quick button per
 * account. Informational only: read-only actions are never blocked.
 */
export function UnboundBanner({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const ctx = useRepoAccount(repoId);
  const accounts = useSortedAccounts();
  const dismissed = useBannerDismissed((s) => s.ids[repoId] === true);
  const openManager = useAccountDialogs((s) => s.openManager);
  const providers = useProviders().data ?? [];
  // Nobody is signed in to the remote's host: signing in there binds the repo by itself.
  const signIn = missingSignIn(ctx.context, accounts, providers);

  // Wait for the context: no flash of the banner while it loads.
  if (dismissed || !ctx.context || ctx.account) return null;

  return (
    <div
      role="status"
      data-testid="unbound-banner"
      className="flex flex-wrap items-center gap-2 border-b bg-warning/10 px-3 py-1.5 text-xs text-warning"
    >
      <UserRoundX className="size-4 shrink-0" />
      <span className="font-medium">{t("account.unbound")}</span>
      <span className="text-muted-foreground">{t("switcher.banner.readOnly")}</span>
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
        {signIn && (
          <Button size="xs" className="gap-1.5" onClick={() => openManager({ signIn })}>
            <LogIn />
            {t("switcher.signIn", { name: signInName(signIn, providers) })}
          </Button>
        )}
        {accounts.slice(0, QUICK).map((a) => (
          <Button
            key={a.id}
            size="xs"
            variant="outline"
            className="gap-1.5 text-foreground"
            onClick={() => void bindRepo(repoId, { accountId: a.id })}
          >
            <AccountAvatar account={a} size={14} />
            {accountLabel(a)}
          </Button>
        ))}
        {accounts.length === 0 && !signIn && (
          <Button
            size="xs"
            variant="outline"
            className="text-foreground"
            onClick={() => openManager({ add: true })}
          >
            <Plus />
            {t("switcher.add")}
          </Button>
        )}
        <Button size="xs" variant="ghost" className="text-foreground" onClick={() => openManager()}>
          <Wand2 />
          {t("switcher.banner.createRule")}
        </Button>
      </div>
      <button
        type="button"
        aria-label={t("common.dismiss")}
        onClick={() => dismissBanner(repoId)}
        className="rounded p-0.5 hover:bg-black/5 dark:hover:bg-white/10"
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}
