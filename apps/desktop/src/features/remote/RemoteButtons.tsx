import { useState } from "react";
import { ArrowDownToLine, ArrowUpFromLine, GitBranchPlus, RefreshCw } from "lucide-react";
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
import { pushPlan } from "./plan";
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
    );
  };

  const pull = (mode: PullMode) => {
    const shot = takeOneShot(repoId, "fetch");
    void startTransfer(
      repoId,
      "pull",
      (op) => commands.remotePull(repoId, op, null, null, mode, asAccountOf(shot)),
      (o) => conflictText(o) || t(`remote.pulled.${o.result}`),
      null,
      shot,
    );
  };

  const gitPush = (p: PushRequest, shot: OneShot | null) =>
    void startTransfer(
      repoId,
      "push",
      (op) => commands.remotePush(repoId, op, p, asAccountOf(shot)),
      (s) =>
        s.upToDate
          ? t("remote.pushUpToDate")
          : t("remote.pushed", { branch: p.branch, remote: p.remote }),
      `${p.remote}/${p.remoteBranch ?? p.branch}`,
      shot,
    );

  /**
   * Push. In an LFS repository: 1) estimate the LFS upload, 2) after confirmation upload the
   * LFS objects with progress (cancel/retry from the transfer queue), 3) `git push`, whose
   * pre-push hook then finds every object on the server already.
   */
  const push = async (force: boolean) => {
    const p = refs ? pushPlan(refs, force) : null;
    if (!p) return;
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
    gitPush(p, shot);
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
        tooltip={t("remote.fetchTip")}
        busy={running?.kind === "fetch"}
        disabled={!hasRemote || busy}
        onClick={fetchAll}
      />
      <ToolButton
        id="pull"
        icon={ArrowDownToLine}
        label={t("toolbar.pull")}
        tooltip={t("remote.pullTip")}
        busy={running?.kind === "pull"}
        disabled={!hasRemote || busy || !!head?.detached}
        onClick={() => pull("merge")}
        menuLabel={t("remote.pullOptions")}
        menu={
          <>
            <DropdownMenuLabel>{t("remote.pullOptions")}</DropdownMenuLabel>
            {PULL_MODES.map((m) => (
              <DropdownMenuItem
                key={m}
                disabled={!hasRemote || busy || !!head?.detached}
                onSelect={() => pull(m)}
              >
                {t(`remote.pullMode.${m}`)}
              </DropdownMenuItem>
            ))}
          </>
        }
      />
      <ToolButton
        id="push"
        icon={ArrowUpFromLine}
        label={t("toolbar.push")}
        tooltip={
          plan
            ? plan.setUpstream
              ? t("remote.pushNewTip", { branch: plan.branch, remote: plan.remote })
              : t("remote.pushTip", { branch: plan.branch, remote: plan.remote })
            : t("remote.pushUnavailable")
        }
        busy={pushing}
        disabled={!plan || busy}
        onClick={() => void push(false)}
        menuLabel={t("remote.pushOptions")}
        menu={
          <>
            <DropdownMenuLabel>{t("remote.pushOptions")}</DropdownMenuLabel>
            <DropdownMenuItem disabled={!plan || busy} onSelect={() => void push(false)}>
              {t("remote.pushNormal")}
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
