import { useState } from "react";
import i18n from "i18next";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  CloudUpload,
  GitBranchPlus,
  RefreshCw,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import {
  accountLabel,
  asAccountOf,
  settleOneShot,
  takeOneShot,
  useRepoAccountSummary,
  useSortedAccounts,
  type OneShot,
} from "@/features/accounts/switcher";
import {
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";
import { ToolButton } from "@/features/shell/ToolButton";
import { conflictText } from "@/features/commits/outcome";
import { useRefDialog } from "@/features/commits/store";
import { useLfsStatus } from "@/features/lfs/queries";
import { useRepoRefs } from "@/features/repos/queries";
import { notifyEstimateFailure, runLfsPush } from "@/features/transfers/actions";
import { failureTexts, formatBytes } from "@/features/transfers/format";
import { errorMessage, commands, type PullMode, type PushEstimate, type PushRequest } from "@/ipc";
import { peekOneShot } from "@/features/accounts/switcher/oneShot";
import { runPushGuard } from "@/features/guards";
import { offerCreatePull } from "@/features/pulls/actions";
import { openPublishDialog } from "@/features/publish/store";
import { useAutoFetch, useLastFetch } from "./autoFetch";
import { pushPlan } from "./plan";
import { useSettings } from "@/stores/settings";
import type { Notice } from "@/stores/ui";
import { startTransfer, useRepoTransfer } from "./transfers";

const PULL_MODES: PullMode[] = ["merge", "rebase", "fastForwardOnly"];

/** Fetch / Pull / Push / Branch buttons of the toolbar for the active repository. */
export function RemoteButtons({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const refs = useRepoRefs(repoId).data;
  const running = useRepoTransfer(repoId);
  const openDialog = useRefDialog((s) => s.open);
  const usesLfs = useLfsStatus(repoId).data?.usesLfs ?? false;
  const account = useRepoAccountSummary(repoId);
  const accounts = useSortedAccounts();
  const [confirmForce, setConfirmForce] = useState(false);
  // Push while the branch misses commits from its upstream: offer to pull first.
  const [behindConfirm, setBehindConfirm] = useState(false);
  const lastFetch = useLastFetch((s) => s.at[repoId] ?? null);
  useAutoFetch(repoId);
  const [estimating, setEstimating] = useState(false);
  const [lfsConfirm, setLfsConfirm] = useState<{
    plan: PushRequest;
    estimate: PushEstimate;
    shot: OneShot | null;
  } | null>(null);
  const hasRemote = !!refs && refs.remotes.length > 0;
  const plan = refs ? pushPlan(refs) : null;
  const busy = running !== null || estimating;
  const pushing = estimating || running?.kind === "push" || running?.kind === "lfsUpload";
  const head = refs?.head;
  const current = refs?.local.find((b) => b.isHead && b.upstream && !b.upstreamGone) ?? null;
  const ahead = current?.ahead ?? 0;
  const behind = current?.behind ?? 0;
  const checkedAt = lastFetch
    ? t("remote.lastChecked", {
        time: new Date(lastFetch).toLocaleTimeString(i18n.language, {
          hour: "2-digit",
          minute: "2-digit",
        }),
      })
    : t("remote.notCheckedYet");

  // One-shot "… as" overrides (switcher) are taken right before each command starts.
  const fetchAll = () => {
    const shot = takeOneShot(repoId, "fetch");
    void startTransfer(
      repoId,
      "fetch",
      (op) => commands.remoteFetch(repoId, op, null, true, true, asAccountOf(shot)),
      (s) =>
        s.updatedRefs ? t("remote.fetched", { count: s.updatedRefs }) : t("remote.fetchedNothing"),
      null,
      shot,
    ).then((s) => {
      if (s) useLastFetch.getState().mark(repoId);
    });
  };

  const pull = (mode: PullMode) => {
    const shot = takeOneShot(repoId, "fetch");
    return startTransfer(
      repoId,
      "pull",
      (op) => commands.remotePull(repoId, op, null, null, mode, asAccountOf(shot)),
      (o) => conflictText(o) || t(`remote.pulled.${o.result}`),
      null,
      shot,
    );
  };

  const gitPush = (p: PushRequest, shot: OneShot | null, pulledFirst = false) => {
    // After a pull the counts in `refs` are stale: say it without a number then.
    const count = p.setUpstream || pulledFirst ? 0 : ahead;
    const target = `${p.remote}/${p.remoteBranch ?? p.branch}`;
    const shotAccount = shot ? accounts.find((a) => a.id === shot.accountId) : undefined;
    const who = shotAccount ? accountLabel(shotAccount) : (account?.label ?? null);
    return void startTransfer(
      repoId,
      "push",
      (op) => commands.remotePush(repoId, op, p, asAccountOf(shot)),
      (s): Notice | string =>
        s.upToDate
          ? t("remote.pushUpToDate")
          : {
              kind: "success",
              message:
                count > 0
                  ? t("remote.pushedCount", { count, branch: p.branch, target })
                  : t("remote.pushed", { branch: p.branch, remote: p.remote }),
              hint:
                [
                  who ? t("remote.pushedAs", { account: who }) : null,
                  useSettings.getState().ciEnabled ? t("remote.pushedHint") : null,
                ]
                  .filter(Boolean)
                  .join(" ") || undefined,
            },
      `${p.remote}/${p.remoteBranch ?? p.branch}`,
      shot,
    ).then((s) => {
      // Phase 4.1: a new branch on GitHub/GitLab → offer "Create PR/MR".
      if (s && !s.upToDate && p.setUpstream) void offerCreatePull(repoId, p.remote, p.branch);
    });
  };

  /**
   * Push. In an LFS repository: 1) estimate the LFS upload, 2) after confirmation upload the
   * LFS objects with progress (cancel/retry from the transfer queue), 3) `git push`, whose
   * pre-push hook then finds every object on the server already.
   */
  const push = async (force: boolean, pulledFirst = false) => {
    const p = refs ? pushPlan(refs, force) : null;
    if (!p) return;
    if (!force && !pulledFirst && behind > 0) {
      setBehindConfirm(true);
      return;
    }
    // Push Guard (PLAN §4.6, §6.3) before anything is uploaded; an explicit "Push as…"
    // override skips it (the guard checks against the repo's own account).
    if (
      !peekOneShot(repoId, "push") &&
      (await runPushGuard(repoId, p.remote, p.branch)) === "cancel"
    ) {
      return;
    }
    // One shot for the whole push (LFS upload + `git push`).
    const shot = takeOneShot(repoId, "push");
    if (usesLfs) {
      setEstimating(true);
      try {
        const est = await commands.lfsPushEstimate(repoId, p.remote, p.branch);
        if (est.status === "done" && est.value.objects.length > 0) {
          setLfsConfirm({ plan: p, estimate: est.value, shot });
          return;
        }
        if (est.status === "failed") {
          notifyEstimateFailure(failureTexts(est.failure, account?.label ?? "").title);
        }
      } catch (e) {
        notifyEstimateFailure(errorMessage(e));
      } finally {
        setEstimating(false);
      }
    }
    gitPush(p, shot, pulledFirst);
  };

  const confirmLfsPush = () => {
    if (!lfsConfirm) return;
    const { plan: p, shot } = lfsConfirm;
    setLfsConfirm(null);
    void runLfsPush(repoId, p.remote, p.branch, () => gitPush(p, shot), null, shot);
  };

  // Who the confirmed LFS upload runs as: the one-shot override, else the repo's account.
  const shotAccount = lfsConfirm?.shot
    ? (accounts.find((a) => a.id === lfsConfirm.shot?.accountId) ?? null)
    : null;
  const uploader = shotAccount
    ? { label: accountLabel(shotAccount), color: shotAccount.color }
    : account;

  const cancelLfsPush = () => {
    // Not pushed: the override stays for the next push.
    settleOneShot(repoId, lfsConfirm?.shot ?? null, false);
    setLfsConfirm(null);
  };

  return (
    <>
      <ToolButton
        id="fetch"
        icon={RefreshCw}
        label={t("toolbar.fetch")}
        tooltip={`${t("remote.fetchTip")}
${checkedAt}`}
        busy={running?.kind === "fetch"}
        disabled={!hasRemote || busy}
        onClick={fetchAll}
      />
      <ToolButton
        id="pull"
        icon={ArrowDownToLine}
        label={t("toolbar.pull")}
        tooltip={
          behind > 0
            ? `${t("remote.behindTip", { count: behind, upstream: current?.upstream ?? "" })}
${checkedAt}`
            : `${t("remote.pullTip")}
${checkedAt}`
        }
        badge={behind > 0 ? { text: `↓${behind}`, tone: ahead > 0 ? "warning" : "info" } : null}
        busy={running?.kind === "pull"}
        disabled={!hasRemote || busy || !!head?.detached}
        onClick={() => void pull("merge")}
        menuLabel={t("remote.pullOptions")}
        menu={
          <>
            <DropdownMenuLabel>{t("remote.pullOptions")}</DropdownMenuLabel>
            {PULL_MODES.map((m) => (
              <DropdownMenuItem
                key={m}
                disabled={!hasRemote || busy || !!head?.detached}
                onSelect={() => void pull(m)}
              >
                {t(`remote.pullMode.${m}`)}
              </DropdownMenuItem>
            ))}
          </>
        }
      />
      {refs && !hasRemote ? (
        // A project that is not on a server yet: create the repository and push in one dialog.
        <ToolButton
          id="push"
          icon={CloudUpload}
          label={t("toolbar.publish")}
          tooltip={t("remote.publishTip")}
          busy={pushing}
          disabled={busy || !!head?.detached}
          onClick={() => openPublishDialog(repoId)}
          menuLabel={t("remote.pushOptions")}
          menu={
            <>
              <DropdownMenuLabel>{t("remote.pushOptions")}</DropdownMenuLabel>
              <DropdownMenuItem onSelect={() => openPublishDialog(repoId)}>
                {t("remote.publishNew")}
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => openPublishDialog(repoId, "url")}>
                {t("remote.addRemoteUrl")}
              </DropdownMenuItem>
            </>
          }
        />
      ) : (
        <ToolButton
          id="push"
          icon={ArrowUpFromLine}
          label={plan?.setUpstream ? t("toolbar.publishBranch") : t("toolbar.push")}
          tooltip={
            plan
              ? plan.setUpstream
                ? t("remote.pushNewTip", { branch: plan.branch, remote: plan.remote })
                : t("remote.pushTip", { branch: plan.branch, remote: plan.remote })
              : head?.unborn
                ? t("remote.pushNoCommit")
                : t("remote.pushUnavailable")
          }
          badge={ahead > 0 ? { text: `↑${ahead}`, tone: behind > 0 ? "warning" : "success" } : null}
          busy={pushing}
          disabled={!plan || busy}
          onClick={() => void push(false)}
          menuLabel={t("remote.pushOptions")}
          menu={
            <>
              <DropdownMenuLabel>{t("remote.pushOptions")}</DropdownMenuLabel>
              <DropdownMenuItem disabled={!plan || busy} onSelect={() => void push(false)}>
                {plan?.setUpstream
                  ? t("remote.publishBranchItem", { branch: plan.branch, remote: plan.remote })
                  : t("remote.pushNormal")}
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => openPublishDialog(repoId, "url")}>
                {t("remote.addRemoteUrl")}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                variant="destructive"
                disabled={!plan || busy}
                onSelect={() => setConfirmForce(true)}
              >
                {t("remote.forcePush")}
              </DropdownMenuItem>
            </>
          }
        />
      )}
      <ToolButton
        id="branch"
        icon={GitBranchPlus}
        label={t("toolbar.branch")}
        tooltip={t("remote.branchTip")}
        disabled={!head?.oid}
        onClick={() =>
          head?.oid &&
          openDialog({
            kind: "createBranch",
            repoId,
            start: head.oid,
            label: head.branch ?? head.oid.slice(0, 7),
          })
        }
      />
      <ConfirmDialog
        open={confirmForce}
        title={t("remote.forceTitle", { branch: plan?.branch ?? "", remote: plan?.remote ?? "" })}
        description={t("remote.forceBody")}
        confirmLabel={t("remote.forcePushConfirm")}
        destructive
        onConfirm={() => {
          setConfirmForce(false);
          void push(true);
        }}
        onCancel={() => setConfirmForce(false)}
      />
      <ConfirmDialog
        open={behindConfirm}
        title={t("remote.behindTitle", { branch: current?.name ?? "", count: behind })}
        description={t("remote.behindBody", {
          upstream: current?.upstream ?? "",
          count: behind,
        })}
        confirmLabel={t("remote.pullThenPush")}
        onConfirm={() => {
          setBehindConfirm(false);
          void pull("merge").then((o) => {
            // Conflicts: the user resolves them first; the push waits.
            if (o && !conflictText(o)) void push(false, true);
          });
        }}
        onCancel={() => setBehindConfirm(false)}
      />
      <ConfirmDialog
        open={lfsConfirm !== null}
        title={t("transfers.confirmTitle")}
        description={
          lfsConfirm && (
            <span>
              {t("transfers.confirmBody", {
                count: lfsConfirm.estimate.objects.length,
                size: formatBytes(lfsConfirm.estimate.totalBytes),
              })}{" "}
              <span className="inline-flex items-center gap-1 font-medium text-foreground">
                <span
                  className="inline-block size-2 rounded-full"
                  style={{ background: uploader?.color ?? "var(--warning)" }}
                />
                {uploader?.label ?? t("transfers.noAccount")}
              </span>
            </span>
          )
        }
        confirmLabel={t("transfers.confirmAction")}
        onConfirm={confirmLfsPush}
        onCancel={cancelLfsPush}
      >
        {lfsConfirm && (
          <div className="space-y-1 text-xs text-muted-foreground">
            {lfsConfirm.estimate.unknownSizes > 0 && (
              <p>{t("transfers.unknownSizes", { count: lfsConfirm.estimate.unknownSizes })}</p>
            )}
            <ul className="max-h-32 overflow-y-auto font-mono text-[11px]">
              {lfsConfirm.estimate.objects.slice(0, 50).map((o) => (
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
        )}
      </ConfirmDialog>
    </>
  );
}
