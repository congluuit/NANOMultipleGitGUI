import {
  ArrowDownToLine,
  Cherry,
  Copy,
  GitBranch,
  GitBranchPlus,
  Link,
  LogIn,
  PencilLine,
  Tag,
  Undo2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/context-menu";
import { parseRefs } from "@/features/graph/draw";
import { RebaseCommitItems } from "@/features/rebase/MenuItems";
import { useRepoRefs } from "@/features/repos/queries";
import { commands, type GraphRow, type Head } from "@/ipc";
import { useHostedRemotes } from "@/features/issues/queries";
import { copyText, startAmend, webCommitUrl } from "./branchActions";
import { conflictText, runOutcome } from "./outcome";
import {
  LocalBranchItems,
  RemoteBranchItems,
  ResetItems,
  RewriteCommitItems,
  TagHereItems,
  TagItems,
} from "./RefMenuItems";
import { useRefDialog } from "./store";

const short = (oid: string) => oid.slice(0, 7);

export function CommitMenuContent({ repoId, row }: { repoId: number; row: GraphRow }) {
  const { t } = useTranslation();
  const refs = useRepoRefs(repoId).data;
  const openDialog = useRefDialog((s) => s.open);
  const head: Head | undefined = refs?.head;
  const isHead = head?.oid === row.hash;
  const current = head?.detached ? null : (head?.branch ?? null);
  const labels = parseRefs(row.refs);
  const locals = labels.filter((l) => l.kind === "branch" || l.kind === "head").map((l) => l.text);
  const remotes = labels.filter((l) => l.kind === "remote" && !l.text.endsWith("/HEAD"));
  const tags = labels.filter((l) => l.kind === "tag").map((l) => l.text);
  const label = `${short(row.hash)} ${row.subject}`;
  const hostedList = useHostedRemotes(repoId).data;
  const hosted = hostedList?.find((h) => h.remote === "origin") ?? hostedList?.[0] ?? null;

  const cherryPick = () =>
    void runOutcome(
      repoId,
      () => commands.commitCherryPick(repoId, row.hash),
      (o) => conflictText(o) || t("ops.cherryPicked", { oid: short(row.hash) }),
    );
  const revert = () =>
    void runOutcome(
      repoId,
      () => commands.commitRevert(repoId, row.hash),
      (o) => conflictText(o) || t("ops.reverted", { oid: short(row.hash) }),
    );
  const checkoutCommit = () =>
    void runOutcome(
      repoId,
      () => commands.commitCheckout(repoId, row.hash),
      () => t("ops.checkedOutCommit", { oid: short(row.hash) }),
    );

  return (
    <ContextMenuContent className="w-72">
      <ContextMenuLabel title={row.subject}>{label}</ContextMenuLabel>
      <ContextMenuSeparator />
      <ContextMenuItem disabled={isHead && !!head?.detached} onSelect={checkoutCommit}>
        <LogIn />
        {t("menu.checkoutCommit")}
      </ContextMenuItem>
      <ContextMenuItem
        onSelect={() =>
          openDialog({ kind: "createBranch", repoId, start: row.hash, label: short(row.hash) })
        }
      >
        <GitBranchPlus />
        {t("menu.createBranch")}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem disabled={isHead} onSelect={cherryPick}>
        <Cherry />
        {t("menu.cherryPick")}
      </ContextMenuItem>
      <ContextMenuItem onSelect={revert}>
        <Undo2 />
        {t("menu.revert")}
      </ContextMenuItem>
      <ResetItems repoId={repoId} oid={row.hash} />
      <RebaseCommitItems repoId={repoId} oid={row.hash} current={current} isHead={isHead} />
      {isHead && (
        <ContextMenuItem onSelect={() => void startAmend(repoId)}>
          <PencilLine />
          {t("menu.amend")}
        </ContextMenuItem>
      )}
      {!isHead && (
        <RewriteCommitItems
          repoId={repoId}
          oid={row.hash}
          isHead={false}
          onCurrentBranch={!!current}
          isMerge={row.isMerge}
        />
      )}

      {(locals.length > 0 || remotes.length > 0 || tags.length > 0) && <ContextMenuSeparator />}
      {locals.map((name) => (
        <ContextMenuSub key={`l:${name}`}>
          <ContextMenuSubTrigger>
            <GitBranch />
            <span className="truncate">{name}</span>
          </ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-64">
            <LocalBranchItems
              repoId={repoId}
              current={current}
              name={name}
              oid={row.hash}
              isMerge={row.isMerge}
            />
          </ContextMenuSubContent>
        </ContextMenuSub>
      ))}
      {remotes.map(({ text }) => (
        <ContextMenuSub key={`r:${text}`}>
          <ContextMenuSubTrigger>
            <ArrowDownToLine />
            <span className="truncate">{text}</span>
          </ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-64">
            <RemoteBranchItems repoId={repoId} current={current} full={text} oid={row.hash} />
          </ContextMenuSubContent>
        </ContextMenuSub>
      ))}
      {tags.map((name) => (
        <ContextMenuSub key={`t:${name}`}>
          <ContextMenuSubTrigger>
            <Tag />
            <span className="truncate">{name}</span>
          </ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-56">
            <TagItems repoId={repoId} name={name} oid={row.hash} />
          </ContextMenuSubContent>
        </ContextMenuSub>
      ))}

      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => copyText(row.hash)}>
        <Copy />
        {t("menu.copySha")}
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => copyText(row.subject)}>
        <Copy />
        {t("menu.copySubject")}
      </ContextMenuItem>
      {hosted && (
        <ContextMenuItem onSelect={() => copyText(webCommitUrl(hosted, row.hash))}>
          <Link />
          {t("branchMenu.copyCommitLink", { remote: hosted.remote })}
        </ContextMenuItem>
      )}
      <ContextMenuSeparator />
      <TagHereItems repoId={repoId} oid={row.hash} />
    </ContextMenuContent>
  );
}
