import { TriangleAlert, UserRound, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useCommitIdentity } from "@/features/worktree/queries";
import type { Account, Identity } from "@/ipc";
import { useAllIdentities, useRepoAccount, useSortedAccounts } from "./hooks";
import { accountLabel, defaultIdentity } from "./model";
import { clearOneShot, oneShotApplies, useOneShot } from "./oneShot";

const sameEmail = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

function AccountLine({
  account,
  identity,
  label,
}: {
  account: Pick<Account, "color">;
  identity: Identity | null;
  label: string;
}) {
  const { t } = useTranslation();
  return (
    <span className="flex min-w-0 flex-1 items-center gap-2">
      <span className="size-2 shrink-0 rounded-full" style={{ background: account.color }} />
      <span className="min-w-0 truncate">
        <span className="font-medium">{label}</span>
        {identity ? (
          <>
            {" · "}
            {identity.name} <span className="text-muted-foreground">&lt;{identity.email}&gt;</span>
          </>
        ) : (
          <span className="text-muted-foreground"> · {t("switcher.noIdentity")}</span>
        )}
      </span>
    </span>
  );
}

/**
 * "Commit với tư cách" block of the commit box (PLAN §7.1): the one-time override when set,
 * else the bound account's identity (warning when the repo's git config has another email),
 * else the git config identity as before.
 */
export function CommitAs({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const git = useCommitIdentity(repoId).data;
  const bound = useRepoAccount(repoId);
  const shot = useOneShot(repoId);
  const accounts = useSortedAccounts();
  const identities = useAllIdentities();
  const commitShot = shot && oneShotApplies(shot, "commit") ? shot : null;
  const shotAccount = commitShot ? accounts.find((a) => a.id === commitShot.accountId) : undefined;
  const shotIdentity = commitShot
    ? commitShot.identityId
      ? (identities.find((i) => i.id === commitShot.identityId) ?? null)
      : defaultIdentity(identities, commitShot.accountId)
    : null;
  const missingGitIdentity = git && (!git.name || !git.email);

  let body: React.ReactNode;
  if (commitShot) {
    body = (
      <div className="flex items-center gap-2 text-xs" data-testid="commit-as-once">
        {shotAccount ? (
          <AccountLine
            account={shotAccount}
            identity={shotIdentity}
            label={accountLabel(shotAccount)}
          />
        ) : (
          <span className="flex-1 truncate">{commitShot.accountId}</span>
        )}
        <span className="shrink-0 rounded bg-primary/10 px-1 text-[10px] text-primary">
          {t("switcher.commitAs.once")}
        </span>
        <button
          type="button"
          aria-label={t("switcher.commitAs.cancel")}
          title={t("switcher.commitAs.cancel")}
          onClick={() => clearOneShot(repoId)}
          className="rounded p-0.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <X className="size-3" />
        </button>
      </div>
    );
  } else if (bound.account) {
    const differs = !!git?.email && !!bound.identity && !sameEmail(git.email, bound.identity.email);
    body = (
      <>
        <div className="flex items-center gap-2 text-xs">
          <AccountLine
            account={bound.account}
            identity={bound.identity}
            label={bound.label ?? ""}
          />
        </div>
        {differs && (
          <div
            className="mt-1 flex items-start gap-1.5 text-[11px] text-warning"
            data-testid="commit-as-differs"
          >
            <TriangleAlert className="mt-0.5 size-3 shrink-0" />
            {t("switcher.commitAs.configDiffers", { email: git.email })}
          </div>
        )}
      </>
    );
  } else if (missingGitIdentity) {
    body = (
      <div className="flex items-start gap-1.5 text-xs text-warning">
        <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
        {t("commit.noIdentity")}
      </div>
    );
  } else {
    body = (
      <>
        <div className="flex items-center gap-2 text-xs">
          <UserRound className="size-3.5 shrink-0 text-muted-foreground" />
          <span className="truncate">
            {git?.name ?? "…"}{" "}
            <span className="text-muted-foreground">&lt;{git?.email ?? "…"}&gt;</span>
          </span>
        </div>
        {bound.context && (
          <div className="mt-1 text-[11px] text-muted-foreground">
            {t("switcher.commitAs.unbound")}
          </div>
        )}
      </>
    );
  }

  return (
    <div className="rounded-md border bg-background p-2" data-testid="commit-as">
      <div className="mb-1 text-[11px] text-muted-foreground">{t("account.commitAs")}</div>
      {body}
    </div>
  );
}
