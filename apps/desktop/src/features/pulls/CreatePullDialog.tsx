import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ArrowUpFromLine, LoaderCircle, TriangleAlert } from "lucide-react";
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
import { push } from "@/features/command-palette/actions";
import { useRepoRefs } from "@/features/repos/queries";
import { commands, type PullCreateContext } from "@/ipc";
import { useHostingNav } from "@/stores/hosting";
import { useUi } from "@/stores/ui";
import { describePullError, notifyPullError } from "./errors";
import { pullNumber, shortTerm, splitLabels, splitNames } from "./model";
import { PullActingAs } from "./parts";
import { invalidatePullLists, pullKeys } from "./queries";
import { usePullUi, type CreatePullRequest } from "./store";

const inputClass =
  "h-8 w-full rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none";

/** Mounted once (App): the "Create pull / merge request" dialog of `usePullUi().create`. */
export function CreatePullDialogHost() {
  const request = usePullUi((s) => s.create);
  const close = usePullUi((s) => s.closeCreate);
  if (!request) return null;
  return (
    <CreatePullDialog
      key={`createPull-${request.repoId}-${request.branch ?? ""}`}
      request={request}
      onClose={close}
    />
  );
}

function Form({
  ctx,
  repoId,
  onClose,
  onPushed,
}: {
  ctx: PullCreateContext;
  repoId: number;
  onClose: () => void;
  onPushed: () => void;
}) {
  const { t } = useTranslation();
  const refs = useRepoRefs(repoId).data;
  const openPull = useHostingNav((s) => s.openPullRequest);
  const family = ctx.target.family;
  const term = shortTerm(family);
  const [title, setTitle] = useState(ctx.suggestedTitle);
  const [body, setBody] = useState(ctx.suggestedBody);
  const [targetBranch, setTargetBranch] = useState(ctx.targetBranch);
  const [draft, setDraft] = useState(false);
  const [reviewers, setReviewers] = useState("");
  const [labels, setLabels] = useState("");
  const [removeSource, setRemoveSource] = useState(true);
  const [actor, setActor] = useState(ctx.target.accountId ?? "");
  const [busy, setBusy] = useState(false);
  const [pushing, setPushing] = useState(false);

  const isCurrent = refs?.head.branch === ctx.branch;
  const needsPush = !ctx.push.pushed || ctx.push.ahead > 0;
  const noAccount = !ctx.target.accountId || !ctx.target.hasToken;
  const canSubmit =
    !busy && !noAccount && !ctx.crossRepoUnsupported && ctx.push.pushed && !!title.trim();

  const doPush = async () => {
    if (!refs) return;
    setPushing(true);
    await push(repoId, refs, false);
    setPushing(false);
    onPushed();
  };

  const submit = async () => {
    if (!canSubmit) return;
    setBusy(true);
    try {
      const created = await commands.pullCreate(actor, ctx.target.fullName, {
        sourceBranch: ctx.sourceRef,
        targetBranch: targetBranch.trim(),
        title: title.trim(),
        body,
        draft,
        reviewers: splitNames(reviewers),
        labels: splitLabels(labels),
        removeSourceBranch: family === "gitlab" && removeSource,
      });
      useUi.getState().showNotice({
        kind: created.warnings.length ? "error" : "info",
        message: t("pulls.create.done", { number: pullNumber(family, created.number), term }),
        hint: created.warnings.length
          ? t("pulls.create.warnings", { warnings: created.warnings.join("; ") })
          : undefined,
      });
      invalidatePullLists();
      onClose();
      openPull({ accountId: actor, fullName: ctx.target.fullName, number: created.number, repoId });
    } catch (e) {
      notifyPullError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <form
        className="flex flex-col gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <div className="flex flex-wrap items-center gap-1 text-xs">
          <span className="rounded bg-surface-2 px-1.5 py-0.5 font-mono">{ctx.sourceRef}</span>
          <span>→</span>
          <span className="text-muted-foreground">{ctx.target.fullName}</span>
          <input
            aria-label={t("pulls.create.targetBranch")}
            value={targetBranch}
            onChange={(e) => setTargetBranch(e.target.value)}
            className="h-6 w-40 rounded border bg-background px-1.5 font-mono text-xs focus:border-primary focus:outline-none"
          />
        </div>
        {needsPush && (
          <div
            className="flex items-start gap-2 rounded-md border border-warning/40 bg-warning/10 p-2 text-xs"
            data-testid="pull-create-push"
          >
            <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-warning" />
            <div className="flex flex-1 flex-col gap-1">
              <span>
                {!ctx.push.pushed
                  ? t("pulls.create.notPushed", {
                      branch: ctx.branch,
                      remote: ctx.push.remote ?? "origin",
                    })
                  : t("pulls.create.ahead", { count: ctx.push.ahead })}
              </span>
              {isCurrent ? (
                <Button
                  size="xs"
                  type="button"
                  className="self-start"
                  disabled={pushing || !refs}
                  onClick={() => void doPush()}
                >
                  {pushing ? <LoaderCircle className="animate-spin" /> : <ArrowUpFromLine />}
                  {t("pulls.create.pushFirst")}
                </Button>
              ) : (
                <span className="text-muted-foreground">
                  {t("pulls.create.checkoutToPush", { branch: ctx.branch })}
                </span>
              )}
            </div>
          </div>
        )}
        {ctx.crossRepoUnsupported && (
          <p className="text-xs text-danger">{t("pulls.create.crossRepoGitlab")}</p>
        )}
        {noAccount && <p className="text-xs text-danger">{t("pulls.create.noAccount")}</p>}
        <label className="text-xs font-semibold" htmlFor="pull-title">
          {t("pulls.create.title")}
        </label>
        <input
          id="pull-title"
          autoFocus
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          className={inputClass}
        />
        <label className="text-xs font-semibold" htmlFor="pull-body">
          {t("pulls.create.body")}
        </label>
        <textarea
          id="pull-body"
          rows={6}
          value={body}
          onChange={(e) => setBody(e.target.value)}
          className="w-full resize-y rounded-md border bg-background px-2 py-1 text-xs focus:border-primary focus:outline-none"
        />
        <div className="grid grid-cols-2 gap-2">
          <label className="flex flex-col gap-0.5 text-xs font-semibold">
            {t("pulls.create.reviewers")}
            <input
              value={reviewers}
              onChange={(e) => setReviewers(e.target.value)}
              placeholder={t("pulls.create.reviewersPlaceholder")}
              className={inputClass}
            />
          </label>
          <label className="flex flex-col gap-0.5 text-xs font-semibold">
            {t("pulls.create.labels")}
            <input
              value={labels}
              onChange={(e) => setLabels(e.target.value)}
              placeholder={t("pulls.create.labelsPlaceholder")}
              className={inputClass}
            />
          </label>
        </div>
        <label className="flex items-center gap-1.5 text-xs">
          <input
            type="checkbox"
            checked={draft}
            onChange={(e) => setDraft(e.target.checked)}
            className="accent-primary"
          />
          {t("pulls.create.draft")}
        </label>
        {family === "gitlab" && (
          <label className="flex items-center gap-1.5 text-xs">
            <input
              type="checkbox"
              checked={removeSource}
              onChange={(e) => setRemoveSource(e.target.checked)}
              className="accent-primary"
            />
            {t("pulls.create.removeSource")}
          </label>
        )}
      </form>
      <DialogFooter className="items-center">
        {actor && <PullActingAs accountId={actor} onChange={setActor} disabled={busy} />}
        <span className="flex-1" />
        <Button variant="outline" size="sm" onClick={onClose}>
          {t("pulls.cancel")}
        </Button>
        <Button size="sm" disabled={!canSubmit} onClick={() => void submit()}>
          {busy && <LoaderCircle className="animate-spin" />}
          {t("pulls.create.submit", { term })}
        </Button>
      </DialogFooter>
    </>
  );
}

function CreatePullDialog({
  request,
  onClose,
}: {
  request: CreatePullRequest;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [targetRemote, setTargetRemote] = useState<string | null>(null);
  const ctx = useQuery({
    queryKey: pullKeys.createContext(request.repoId, request.branch, targetRemote),
    queryFn: () => commands.pullCreateContext(request.repoId, request.branch, targetRemote, null),
    staleTime: 0,
    retry: false,
  });
  const term = shortTerm(ctx.data?.target.family);
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{t("pulls.create.dialogTitle", { term })}</DialogTitle>
          <DialogDescription>
            {ctx.data
              ? t("pulls.create.dialogDescription", {
                  branch: ctx.data.branch,
                  count: ctx.data.commitCount,
                })
              : t("pulls.loading")}
          </DialogDescription>
        </DialogHeader>
        {ctx.data && ctx.data.remotes.length > 1 && (
          <label className="flex items-center gap-2 text-xs">
            {t("pulls.create.targetRemote")}
            <select
              value={ctx.data.target.remote}
              onChange={(e) => setTargetRemote(e.target.value)}
              className="h-7 rounded-md border bg-background px-1 text-xs"
            >
              {ctx.data.remotes.map((r) => (
                <option key={r.remote} value={r.remote}>
                  {r.remote} — {r.fullName}
                </option>
              ))}
            </select>
          </label>
        )}
        {ctx.isPending && (
          <div className="flex items-center gap-2 py-6 text-xs text-muted-foreground">
            <LoaderCircle className="size-4 animate-spin" />
            {t("pulls.loading")}
          </div>
        )}
        {ctx.isError && (
          <p className="text-xs text-danger" role="alert">
            {describePullError(ctx.error).message}
          </p>
        )}
        {ctx.data && (
          <Form
            key={`form-${ctx.data.target.remote}`}
            ctx={ctx.data}
            repoId={request.repoId}
            onClose={onClose}
            onPushed={() => void ctx.refetch()}
          />
        )}
        {!ctx.data && (
          <DialogFooter>
            <Button variant="outline" size="sm" onClick={onClose}>
              {t("pulls.cancel")}
            </Button>
            <Button size="sm" disabled>
              {t("pulls.create.submit", { term })}
            </Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  );
}
