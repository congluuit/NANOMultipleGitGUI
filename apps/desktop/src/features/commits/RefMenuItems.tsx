import {
  ArrowDownToLine,
  ArrowUpFromLine,
  Copy,
  FolderGit2,
  GitBranchPlus,
  GitMerge,
  History,
  Link,
  LogIn,
  PencilLine,
  Tag,
  Trash2,
  Undo2,
  Unlink,
  Upload,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/context-menu";
import { useHostedRemotes } from "@/features/issues/queries";
import { openCreatePull } from "@/features/pulls/store";
import { RebaseBranchItems } from "@/features/rebase/MenuItems";
import { openRebaseDialog } from "@/features/rebase/store";
import { splitUpstream } from "@/features/remote/plan";
import { useRepoRefs } from "@/features/repos/queries";
import { commands, type HostedRemote, type RepoRefs } from "@/ipc";
import {
  copyText,
  pullBranch,
  pushBranch,
  setUpstream,
  startAmend,
  webBranchUrl,
  webCommitUrl,
} from "./branchActions";
import { conflictText, runOutcome } from "./outcome";
import { useRefDialog } from "./store";

interface Common {
  repoId: number;
  /** Checked-out branch, `null` when HEAD is detached. */
  current: string | null;
}

const short = (oid: string) => oid.slice(0, 7);

/** Hosted project of `remote` (falls back to the first hosted remote). */
function hostedFor(list: readonly HostedRemote[] | undefined, remote: string | null) {
  if (!list?.length) return null;
  return list.find((h) => h.remote === remote) ?? (remote ? null : list[0]!);
}

/** "Reset <current> to this commit ▸" soft / mixed / hard. */
export function ResetItems({ repoId, oid }: { repoId: number; oid: string }) {
  const { t } = useTranslation();
  const refs = useRepoRefs(repoId).data;
  const open = useRefDialog((s) => s.open);
  const head = refs?.head;
  const current = head?.detached ? null : (head?.branch ?? null);
  const reset = (mode: "soft" | "mixed") =>
    void runOutcome(
      repoId,
      () => commands.resetTo(repoId, oid, mode),
      () => t("ops.resetDone", { oid: short(oid), mode }),
    );
  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger disabled={!head || head.unborn || head.oid === oid}>
        <History />
        {current ? t("menu.resetBranch", { branch: current }) : t("menu.resetHead")}
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="w-64">
        <ContextMenuItem onSelect={() => reset("soft")}>{t("menu.resetSoft")}</ContextMenuItem>
        <ContextMenuItem onSelect={() => reset("mixed")}>{t("menu.resetMixed")}</ContextMenuItem>
        <ContextMenuItem
          variant="destructive"
          onSelect={() =>
            open({ kind: "resetHard", repoId, oid, label: short(oid), branch: current })
          }
        >
          {t("menu.resetHard")}
        </ContextMenuItem>
      </ContextMenuSubContent>
    </ContextMenuSub>
  );
}

/** "Edit commit message" + "Drop commit" for a commit of the checked-out branch. */
export function RewriteCommitItems({
  repoId,
  oid,
  isHead,
  onCurrentBranch,
  isMerge = false,
}: {
  repoId: number;
  oid: string;
  isHead: boolean;
  /** The commit is in the history of the checked-out branch (rewriting it is possible). */
  onCurrentBranch: boolean;
  isMerge?: boolean;
}) {
  const { t } = useTranslation();
  const rewrite = (action: "drop" | "reword") =>
    openRebaseDialog({
      kind: "interactive",
      repoId,
      base: `${oid}~1`,
      baseLabel: `${short(oid)}~1`,
      preset: { oid, action },
    });
  return (
    <>
      <ContextMenuItem
        disabled={!onCurrentBranch || isMerge}
        onSelect={() => (isHead ? void startAmend(repoId) : rewrite("reword"))}
      >
        <PencilLine />
        {t("branchMenu.editMessage")}
      </ContextMenuItem>
      <ContextMenuItem
        variant="destructive"
        disabled={!onCurrentBranch || isMerge}
        onSelect={() => rewrite("drop")}
      >
        <X />
        {t("branchMenu.dropCommit")}
      </ContextMenuItem>
    </>
  );
}

/** "Create tag here" / "Create annotated tag here". */
export function TagHereItems({ repoId, oid }: { repoId: number; oid: string }) {
  const { t } = useTranslation();
  const open = useRefDialog((s) => s.open);
  return (
    <>
      <ContextMenuItem onSelect={() => open({ kind: "createTag", repoId, oid, label: short(oid) })}>
        <Tag />
        {t("branchMenu.createTag")}
      </ContextMenuItem>
      <ContextMenuItem
        onSelect={() =>
          open({ kind: "createTag", repoId, oid, label: short(oid), annotated: true })
        }
      >
        <Tag />
        {t("branchMenu.createAnnotatedTag")}
      </ContextMenuItem>
    </>
  );
}

function SetUpstreamSub({ repoId, refs, name }: { repoId: number; refs: RepoRefs; name: string }) {
  const { t } = useTranslation();
  const local = refs.local.find((b) => b.name === name);
  const candidates = refs.remotes.flatMap((r) =>
    r.branches.filter((b) => b.name !== "HEAD").map((b) => `${r.name}/${b.name}`),
  );
  // Same-name branches first.
  candidates.sort(
    (a, b) => Number(b.endsWith(`/${name}`)) - Number(a.endsWith(`/${name}`)) || a.localeCompare(b),
  );
  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger disabled={refs.remotes.length === 0}>
        <Link />
        {t("branchMenu.setUpstream")}
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="max-h-80 w-64 overflow-y-auto">
        {candidates.length === 0 && (
          <ContextMenuLabel className="font-normal text-muted-foreground">
            {t("branchMenu.noRemoteBranches")}
          </ContextMenuLabel>
        )}
        {candidates.slice(0, 50).map((c) => (
          <ContextMenuItem
            key={c}
            disabled={local?.upstream === c}
            onSelect={() => void setUpstream(repoId, name, c)}
          >
            <span className="truncate">{c}</span>
          </ContextMenuItem>
        ))}
        {local?.upstream && (
          <>
            <ContextMenuSeparator />
            <ContextMenuItem onSelect={() => void setUpstream(repoId, name, null)}>
              <Unlink />
              {t("branchMenu.unsetUpstream")}
            </ContextMenuItem>
          </>
        )}
      </ContextMenuSubContent>
    </ContextMenuSub>
  );
}

/**
 * Menu of a local branch (sidebar row, graph label, commit menu submenu), laid out like other
 * Git clients: sync, checkout, the tip commit, merge/rebase, PR, rename/delete, copy, tags.
 */
export function LocalBranchItems({
  repoId,
  current,
  name,
  oid: givenOid,
  isMerge = false,
}: Common & { name: string; oid?: string; isMerge?: boolean }) {
  const { t } = useTranslation();
  const open = useRefDialog((s) => s.open);
  const refs = useRepoRefs(repoId).data;
  const hosted = useHostedRemotes(repoId).data;
  const isCurrent = name === current;
  const local = refs?.local.find((b) => b.name === name);
  const oid = givenOid ?? local?.oid;
  const headOid = refs?.head.oid ?? null;
  const up =
    refs && local?.upstream && !local.upstreamGone
      ? splitUpstream(
          local.upstream,
          refs.remotes.map((r) => r.name),
        )
      : null;
  const remoteName =
    up?.remote ?? (refs?.remotes.some((r) => r.name === "origin") ? "origin" : null);
  const h = hostedFor(hosted, remoteName);

  return (
    <>
      <ContextMenuItem
        disabled={!refs || !local}
        onSelect={() => refs && void pullBranch(repoId, refs, name)}
      >
        <ArrowDownToLine />
        {t("branchMenu.pull")}
      </ContextMenuItem>
      <ContextMenuItem
        disabled={!refs || refs.remotes.length === 0}
        onSelect={() => refs && void pushBranch(repoId, refs, name)}
      >
        <ArrowUpFromLine />
        {t("branchMenu.push")}
      </ContextMenuItem>
      {refs && <SetUpstreamSub repoId={repoId} refs={refs} name={name} />}
      <ContextMenuSeparator />

      <ContextMenuItem
        disabled={isCurrent}
        onSelect={() =>
          void runOutcome(
            repoId,
            () => commands.branchCheckout(repoId, name),
            () => t("ops.checkedOut", { name }),
          )
        }
      >
        <LogIn />
        {t("branchMenu.checkout", { name })}
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => open({ kind: "addWorktree", repoId, branch: name })}>
        <FolderGit2 />
        {t("branchMenu.worktree", { name })}
      </ContextMenuItem>
      <ContextMenuSeparator />

      {oid && (
        <>
          <ContextMenuItem
            onSelect={() => open({ kind: "createBranch", repoId, start: oid, label: name })}
          >
            <GitBranchPlus />
            {t("branchMenu.createBranchHere")}
          </ContextMenuItem>
          <ResetItems repoId={repoId} oid={oid} />
          <RewriteCommitItems
            repoId={repoId}
            oid={oid}
            isHead={oid === headOid}
            onCurrentBranch={isCurrent}
            isMerge={isMerge}
          />
          <ContextMenuItem
            onSelect={() =>
              void runOutcome(
                repoId,
                () => commands.commitRevert(repoId, oid),
                (o) => conflictText(o) || t("ops.reverted", { oid: short(oid) }),
              )
            }
          >
            <Undo2 />
            {t("menu.revert")}
          </ContextMenuItem>
          <ContextMenuSeparator />
        </>
      )}

      <ContextMenuItem
        disabled={isCurrent || !current}
        onSelect={() => open({ kind: "merge", repoId, rev: name, into: current })}
      >
        <GitMerge />
        {t("menu.mergeInto", { branch: current ?? "HEAD" })}
      </ContextMenuItem>
      <RebaseBranchItems repoId={repoId} name={name} current={current} />
      <ContextMenuItem disabled={!h} onSelect={() => openCreatePull(repoId, name)}>
        <Upload />
        {local?.upstream
          ? t("branchMenu.startPull", { name })
          : t("branchMenu.pushAndStartPull", { name })}
      </ContextMenuItem>
      <ContextMenuSeparator />

      <ContextMenuItem onSelect={() => open({ kind: "renameBranch", repoId, name })}>
        <PencilLine />
        {t("branchMenu.rename", { name })}
      </ContextMenuItem>
      <ContextMenuItem
        variant="destructive"
        disabled={isCurrent}
        onSelect={() => open({ kind: "deleteBranch", repoId, name, force: false })}
      >
        <Trash2 />
        {t("branchMenu.delete", { name })}
      </ContextMenuItem>
      {up && (
        <>
          <ContextMenuItem
            variant="destructive"
            onSelect={() =>
              open({
                kind: "deleteRemoteBranch",
                repoId,
                remote: up.remote,
                branch: up.branch,
                local: null,
              })
            }
          >
            <Trash2 />
            {t("branchMenu.delete", { name: local!.upstream })}
          </ContextMenuItem>
          <ContextMenuItem
            variant="destructive"
            disabled={isCurrent}
            onSelect={() =>
              open({
                kind: "deleteRemoteBranch",
                repoId,
                remote: up.remote,
                branch: up.branch,
                local: name,
              })
            }
          >
            <Trash2 />
            {t("branchMenu.deleteBoth", { name, remote: local!.upstream })}
          </ContextMenuItem>
        </>
      )}
      <ContextMenuSeparator />

      <ContextMenuItem onSelect={() => copyText(name)}>
        <Copy />
        {t("branchMenu.copyName")}
      </ContextMenuItem>
      {oid && (
        <ContextMenuItem onSelect={() => copyText(oid)}>
          <Copy />
          {t("menu.copySha")}
        </ContextMenuItem>
      )}
      {h && up && (
        <ContextMenuItem onSelect={() => copyText(webBranchUrl(h, up.branch))}>
          <Link />
          {t("branchMenu.copyBranchLink", { name: local!.upstream })}
        </ContextMenuItem>
      )}
      {h && oid && (
        <ContextMenuItem onSelect={() => copyText(webCommitUrl(h, oid))}>
          <Link />
          {t("branchMenu.copyCommitLink", { remote: h.remote })}
        </ContextMenuItem>
      )}
      {oid && (
        <>
          <ContextMenuSeparator />
          <TagHereItems repoId={repoId} oid={oid} />
        </>
      )}
    </>
  );
}

/** Items for a remote-tracking branch, `full` being `origin/feature/x`. */
export function RemoteBranchItems({
  repoId,
  current,
  full,
  oid: givenOid,
}: Common & { full: string; oid?: string }) {
  const { t } = useTranslation();
  const open = useRefDialog((s) => s.open);
  const refs = useRepoRefs(repoId).data;
  const hosted = useHostedRemotes(repoId).data;
  const split = refs
    ? splitUpstream(
        full,
        refs.remotes.map((r) => r.name),
      )
    : null;
  const slash = full.indexOf("/");
  const remote = split?.remote ?? full.slice(0, slash);
  const branch = split?.branch ?? full.slice(slash + 1);
  const oid =
    givenOid ??
    refs?.remotes.find((r) => r.name === remote)?.branches.find((b) => b.name === branch)?.oid;
  const h = hostedFor(hosted, remote);
  return (
    <>
      <ContextMenuItem
        onSelect={() =>
          void runOutcome(
            repoId,
            () => commands.branchCheckoutRemote(repoId, remote, branch, null),
            (name) => t("ops.checkedOut", { name }),
          )
        }
      >
        <LogIn />
        {t("menu.checkoutRemote")}
      </ContextMenuItem>
      {oid && (
        <ContextMenuItem
          onSelect={() => open({ kind: "createBranch", repoId, start: oid, label: full })}
        >
          <GitBranchPlus />
          {t("branchMenu.createBranchHere")}
        </ContextMenuItem>
      )}
      {oid && <ResetItems repoId={repoId} oid={oid} />}
      <ContextMenuSeparator />
      <ContextMenuItem
        disabled={!current}
        onSelect={() => open({ kind: "merge", repoId, rev: full, into: current })}
      >
        <GitMerge />
        {t("menu.mergeInto", { branch: current ?? "HEAD" })}
      </ContextMenuItem>
      <RebaseBranchItems repoId={repoId} name={full} current={current} />
      <ContextMenuSeparator />
      <ContextMenuItem
        variant="destructive"
        onSelect={() => open({ kind: "deleteRemoteBranch", repoId, remote, branch, local: null })}
      >
        <Trash2 />
        {t("branchMenu.delete", { name: full })}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => copyText(full)}>
        <Copy />
        {t("branchMenu.copyName")}
      </ContextMenuItem>
      {oid && (
        <ContextMenuItem onSelect={() => copyText(oid)}>
          <Copy />
          {t("menu.copySha")}
        </ContextMenuItem>
      )}
      {h && (
        <ContextMenuItem onSelect={() => copyText(webBranchUrl(h, branch))}>
          <Link />
          {t("branchMenu.copyBranchLink", { name: full })}
        </ContextMenuItem>
      )}
      {h && oid && (
        <ContextMenuItem onSelect={() => copyText(webCommitUrl(h, oid))}>
          <Link />
          {t("branchMenu.copyCommitLink", { remote: h.remote })}
        </ContextMenuItem>
      )}
      {oid && (
        <>
          <ContextMenuSeparator />
          <TagHereItems repoId={repoId} oid={oid} />
        </>
      )}
    </>
  );
}

export function TagItems({ repoId, name, oid }: { repoId: number; name: string; oid?: string }) {
  const { t } = useTranslation();
  const open = useRefDialog((s) => s.open);
  return (
    <>
      {oid && (
        <ContextMenuItem
          onSelect={() => open({ kind: "createBranch", repoId, start: oid, label: name })}
        >
          <GitBranchPlus />
          {t("menu.branchFrom", { name })}
        </ContextMenuItem>
      )}
      <ContextMenuItem
        variant="destructive"
        onSelect={() => open({ kind: "deleteTag", repoId, name })}
      >
        <Trash2 />
        {t("menu.deleteTag")}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => copyText(name)}>
        <Copy />
        {t("menu.copyName")}
      </ContextMenuItem>
    </>
  );
}
