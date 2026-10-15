import { useState } from "react";
import { ChevronDown, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { commands } from "@/ipc";
import { useHostingNav } from "@/stores/hosting";
import { useUi } from "@/stores/ui";
import { ActingAs } from "./ActingAs";
import { issueErrorNotice } from "./actions";
import { IssueContextHint } from "./context";
import { useIssueContext } from "./useIssueContext";
import { LabelDot } from "./IssueSidebarList";
import { invalidateIssueLists, useAssignableUsers, useIssueLabels } from "./queries";
import { useIssueUi } from "./store";

/** Mounted once (App): the "New issue" dialog of the repository in `useIssueUi().createFor`. */
export function IssueCreateHost() {
  const repoId = useIssueUi((s) => s.createFor);
  const close = useIssueUi((s) => s.closeCreate);
  return (
    <Dialog open={repoId !== null} onOpenChange={(o) => !o && close()}>
      {repoId !== null && (
        <IssueCreateForm key={`issueCreate-${repoId}`} repoId={repoId} onDone={close} />
      )}
    </Dialog>
  );
}

function toggle(list: string[], item: string) {
  return list.includes(item) ? list.filter((x) => x !== item) : [...list, item];
}

function IssueCreateForm({ repoId, onDone }: { repoId: number; onDone: () => void }) {
  const { t } = useTranslation();
  const ctx = useIssueContext(repoId);
  const ready = ctx.status === "ready";
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [labels, setLabels] = useState<string[]>([]);
  const [assignees, setAssignees] = useState<string[]>([]);
  const [actor, setActor] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const accountId = ready ? ctx.accountId : null;
  const fullName = ready ? ctx.remote.fullName : null;
  const allLabels = useIssueLabels(accountId, fullName);
  const users = useAssignableUsers(accountId, fullName);
  const acting = actor ?? accountId;

  const submit = async () => {
    if (!ready || !title.trim() || !acting) return;
    setBusy(true);
    try {
      const made = await commands.issueCreate(
        repoId,
        ctx.remote.remote,
        acting !== ctx.accountId ? acting : null,
        { title: title.trim(), body, labels, assignees },
      );
      invalidateIssueLists();
      useUi.getState().showNotice({
        kind: "info",
        message: t("issues.create.done", {
          number: made.issue.issue.number,
          title: made.issue.issue.title,
        }),
      });
      useHostingNav.getState().openIssue({
        accountId: made.accountId,
        fullName: made.fullName,
        number: made.issue.issue.number,
        repoId,
      });
      onDone();
    } catch (e) {
      useUi.getState().showNotice(issueErrorNotice(e, acting));
    } finally {
      setBusy(false);
    }
  };

  return (
    <DialogContent className="sm:max-w-lg">
      <form
        className="grid gap-3 text-xs"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <DialogHeader>
          <DialogTitle>{t("issues.create.title")}</DialogTitle>
          <DialogDescription>
            {ready ? t("issues.create.description", { project: ctx.remote.fullName }) : ""}
          </DialogDescription>
        </DialogHeader>
        {!ready ? (
          <IssueContextHint state={ctx} />
        ) : (
          <>
            <label className="grid gap-1">
              <span className="font-medium">{t("issues.create.titleLabel")}</span>
              <input
                autoFocus
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                className="h-8 rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none"
              />
            </label>
            <label className="grid gap-1">
              <span className="font-medium">{t("issues.create.bodyLabel")}</span>
              <textarea
                value={body}
                onChange={(e) => setBody(e.target.value)}
                rows={6}
                placeholder={t("issues.create.bodyPlaceholder")}
                className="resize-y rounded-md border bg-background px-2 py-1.5 text-xs focus:border-primary focus:outline-none"
              />
            </label>
            <div className="flex flex-wrap gap-2">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button type="button" size="xs" variant="outline">
                    {labels.length
                      ? t("issues.create.labelsCount", { count: labels.length })
                      : t("issues.view.labels")}
                    <ChevronDown />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent className="max-h-72 w-56 overflow-y-auto text-xs">
                  {allLabels.isPending && (
                    <DropdownMenuItem disabled>{t("issues.loading")}</DropdownMenuItem>
                  )}
                  {allLabels.data?.length === 0 && (
                    <DropdownMenuItem disabled>{t("issues.filter.noLabels")}</DropdownMenuItem>
                  )}
                  {(allLabels.data ?? []).map((l) => (
                    <DropdownMenuCheckboxItem
                      key={`cl-${l.name}`}
                      checked={labels.includes(l.name)}
                      onSelect={(e) => e.preventDefault()}
                      onCheckedChange={() => setLabels((x) => toggle(x, l.name))}
                    >
                      <LabelDot color={l.color} />
                      <span className="truncate">{l.name}</span>
                    </DropdownMenuCheckboxItem>
                  ))}
                </DropdownMenuContent>
              </DropdownMenu>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button type="button" size="xs" variant="outline">
                    {assignees.length
                      ? t("issues.create.assigneesCount", { count: assignees.length })
                      : t("issues.view.assignees")}
                    <ChevronDown />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent className="max-h-72 w-56 overflow-y-auto text-xs">
                  {users.isPending && (
                    <DropdownMenuItem disabled>{t("issues.loading")}</DropdownMenuItem>
                  )}
                  {users.isError && (
                    <DropdownMenuItem disabled>{t("issues.view.cannotLoad")}</DropdownMenuItem>
                  )}
                  {(users.data ?? []).map((u) => (
                    <DropdownMenuCheckboxItem
                      key={`ca-${u.username}`}
                      checked={assignees.includes(u.username)}
                      onSelect={(e) => e.preventDefault()}
                      onCheckedChange={() => setAssignees((x) => toggle(x, u.username))}
                    >
                      <span className="truncate">{u.username}</span>
                    </DropdownMenuCheckboxItem>
                  ))}
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </>
        )}
        <DialogFooter className="items-center gap-2 sm:justify-between">
          <div className="min-w-0">
            {ready && acting && <ActingAs accountId={acting} onChange={setActor} disabled={busy} />}
          </div>
          <div className="flex gap-2">
            <Button type="button" variant="ghost" size="sm" onClick={onDone}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={!ready || busy || !title.trim()}>
              {busy && <LoaderCircle className="animate-spin" />}
              {t("issues.create.submit")}
            </Button>
          </div>
        </DialogFooter>
      </form>
    </DialogContent>
  );
}
