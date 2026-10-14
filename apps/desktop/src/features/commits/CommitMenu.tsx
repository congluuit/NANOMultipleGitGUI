import {
  ArrowDownToLine,
  Cherry,
  Copy,
  GitBranch,
  GitBranchPlus,
  History,
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
import { useRepoRefs } from "@/features/repos/queries";
import { commands, type GraphRow, type Head } from "@/ipc";
import { useGraphStore } from "@/stores/graph";
import { useWorktreeStore } from "@/stores/worktree";
import { conflictText, runOutcome } from "./outcome";
import { LocalBranchItems, RemoteBranchItems, TagItems } from "./RefMenuItems";
import { useRefDialog } from "./store";

const short = (oid: string) => oid.slice(0, 7);

/** Puts the staging panel in amend mode with the last commit's message. */
async function startAmend(repoId: number) {
  useGraphStore.getState().select(repoId, null);
  const last = await commands.headMessage(repoId).catch(() => null);
  const [summary = "", ...rest] = (last ?? "").split("\n");
  useWorktreeStore
    .getState()
    .updateDraft(repoId, { amend: true, summary, body: rest.join("\n").trim(), prefill: last });
}

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
  const reset = (mode: "soft" | "mixed") =>
    void runOutcome(
      repoId,
      () => commands.resetTo(repoId, row.hash, mode),
      () => t("ops.resetDone", { oid: short(row.hash), mode }),
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
      <ContextMenuItem
        onSelect={() =>
          openDialog({ kind: "createTag", repoId, oid: row.hash, label: short(row.hash) })
        }
      >
        <Tag />
        {t("menu.createTag")}
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
      <ContextMenuSub>
        <ContextMenuSubTrigger disabled={!head || head.unborn}>
          <History />
          {current ? t("menu.resetBranch", { branch: current }) : t("menu.resetHead")}
        </ContextMenuSubTrigger>
        <ContextMenuSubContent className="w-64">
          <ContextMenuItem onSelect={() => reset("soft")}>{t("menu.resetSoft")}</ContextMenuItem>
          <ContextMenuItem onSelect={() => reset("mixed")}>{t("menu.resetMixed")}</ContextMenuItem>
          <ContextMenuItem
            variant="destructive"
            onSelect={() =>
              openDialog({
                kind: "resetHard",
                repoId,
                oid: row.hash,
                label: short(row.hash),
                branch: current,
              })
            }
          >
            {t("menu.resetHard")}
          </ContextMenuItem>
        </ContextMenuSubContent>
      </ContextMenuSub>
      {isHead && (
        <ContextMenuItem onSelect={() => void startAmend(repoId)}>
          <PencilLine />
          {t("menu.amend")}
        </ContextMenuItem>
      )}

      {(locals.length > 0 || remotes.length > 0 || tags.length > 0) && <ContextMenuSeparator />}
      {locals.map((name) => (
        <ContextMenuSub key={`l:${name}`}>
          <ContextMenuSubTrigger>
            <GitBranch />
            <span className="truncate">{name}</span>
          </ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-64">
            <LocalBranchItems repoId={repoId} current={current} name={name} />
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
            <RemoteBranchItems repoId={repoId} current={current} full={text} />
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
            <TagItems repoId={repoId} name={name} />
          </ContextMenuSubContent>
        </ContextMenuSub>
      ))}

      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => void navigator.clipboard.writeText(row.hash)}>
        <Copy />
        {t("menu.copySha")}
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => void navigator.clipboard.writeText(row.subject)}>
        <Copy />
        {t("menu.copySubject")}
      </ContextMenuItem>
    </ContextMenuContent>
  );
}
