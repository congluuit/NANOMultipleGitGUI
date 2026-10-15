import { useMemo, useState } from "react";
import { Check, CloudUpload, LoaderCircle, UserPlus } from "lucide-react";
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { openExternal } from "@/features/accounts/manager/openExternal";
import { useAccounts, useProviders } from "@/features/accounts/queries";
import { accountLabel } from "@/features/accounts/switcher/model";
import type { DescribedProblem } from "@/features/ci/errors";
import {
  accountsOnHost,
  describeForkFailure,
  freeRemoteName,
  validRemoteName,
} from "@/features/forks/model";
import { NamespaceSelect } from "@/features/forks/NamespaceSelect";
import { useRepoRefs } from "@/features/repos/queries";
import type { Account, ForkRemoteChange, ForkRepo } from "@/ipc";
import { cn } from "@/lib/utils";
import { useWorkspace } from "@/stores/workspace";
import { publishNewRepo, publishToUrl, type PublishPhase, type PublishStep } from "./actions";
import {
  firstSteps,
  plausibleRemoteUrl,
  remoteHost,
  repoUrl,
  suggestRepoName,
  validRepoName,
} from "./model";
import { usePublishDialog, type PublishMode } from "./store";

const inputClass =
  "h-8 w-full rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none aria-invalid:border-danger";

/** Account "from the rules" in URL mode (the resolver picks one by host / rules). */
const AUTO = "__auto__";

/** Mounted once (App.tsx); opens through `openPublishDialog(repoId, mode?)`. */
export function PublishDialogHost() {
  const preset = usePublishDialog((s) => s.preset);
  const close = usePublishDialog((s) => s.close);
  if (!preset) return null;
  return (
    <PublishDialog
      key={`publish-${preset.repoId}-${preset.mode}`}
      repoId={preset.repoId}
      initialMode={preset.mode}
      onClose={close}
    />
  );
}

function StepDot({ n, state }: { n: number; state: "done" | "current" | "todo" }) {
  return (
    <span
      className={cn(
        "inline-flex size-5 shrink-0 items-center justify-center rounded-full border text-[10px] font-semibold",
        state === "done" && "border-success bg-success text-white",
        state === "current" && "border-primary text-primary",
        state === "todo" && "text-muted-foreground",
      )}
    >
      {state === "done" ? <Check className="size-3" /> : n}
    </span>
  );
}

/**
 * "Publish repository…": for a project that is not on a server yet, create an empty repository
 * on GitHub / GitLab with one of the accounts (or paste the URL of an existing one), add it as
 * `origin` bound to that account and push the current branch — in one dialog, step by step.
 */
export function PublishDialog({
  repoId,
  initialMode,
  onClose,
}: {
  repoId: number;
  initialMode: PublishMode;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  const tab = useWorkspace((s) => s.tabs.find((x) => x.id === repoId));
  const refs = useRepoRefs(repoId).data;
  const accounts = useAccounts().data;
  const providers = useProviders().data;
  const withToken = useMemo(
    () => (accounts ?? []).filter((a) => a.secrets.includes("token")),
    [accounts],
  );

  const [mode, setMode] = useState<PublishMode>(initialMode);
  const [accountChoice, setAccountChoice] = useState<string | null>(null);
  const account: Account | undefined =
    withToken.find((a) => a.id === accountChoice) ?? withToken[0];
  const provider = providers?.find((p) => p.id === account?.providerId);
  const [namespace, setNamespace] = useState<string | null>(null);
  const [name, setName] = useState(() => suggestRepoName(tab?.path ?? tab?.name ?? ""));
  const [description, setDescription] = useState("");
  const [isPrivate, setIsPrivate] = useState(true);
  const [protocol, setProtocol] = useState<"https" | "ssh">("https");

  const [url, setUrl] = useState("");
  const urlHost = remoteHost(url);
  const urlAccounts = useMemo(
    () => accountsOnHost(accounts ?? [], providers, urlHost),
    [accounts, providers, urlHost],
  );
  const [urlAccountChoice, setUrlAccountChoice] = useState<string | null>(null);
  const urlAccountId =
    urlAccountChoice === AUTO
      ? null
      : (urlAccounts.find((a) => a.id === urlAccountChoice)?.id ?? urlAccounts[0]?.id ?? null);

  const existingRemotes = useMemo(() => refs?.remotes.map((r) => r.name) ?? [], [refs]);
  const [remoteEdit, setRemoteEdit] = useState<string | null>(null);
  const remoteName = (remoteEdit ?? freeRemoteName(existingRemotes, "origin")).trim();
  const remoteInvalid = !validRemoteName(remoteName) || existingRemotes.includes(remoteName);

  const steps = refs ? firstSteps(refs) : null;
  const branch = refs?.head.detached ? null : (refs?.head.branch ?? null);
  const canPush = !!branch && !!refs && !refs.head.unborn;
  const [pushAfter, setPushAfter] = useState(true);

  const [phase, setPhase] = useState<PublishPhase | null>(null);
  const [problem, setProblem] = useState<DescribedProblem | null>(null);
  const [result, setResult] = useState<{
    repo: ForkRepo | null;
    change: ForkRemoteChange;
    pushed: boolean;
    pushFailed: boolean;
  } | null>(null);
  const running = phase !== null;
  const locked = running || result !== null;

  const nameInvalid = !validRepoName(name.trim());
  const urlInvalid = !plausibleRemoteUrl(url);
  const canSubmit =
    !locked &&
    !remoteInvalid &&
    (mode === "new" ? !!account && !nameInvalid : !urlInvalid) &&
    !!refs;

  const finish = (step: PublishStep, accountId: string | null) => {
    switch (step.status) {
      case "done":
        setResult({ repo: step.repo, change: step.change, pushed: step.pushed, pushFailed: false });
        return;
      case "pushFailed":
        setResult({ repo: step.repo, change: step.change, pushed: false, pushFailed: true });
        return;
      case "failed":
        setProblem({ text: step.message, action: null });
        return;
      case "createFailed": {
        const f = step.failure;
        const acc = withToken.find((a) => a.id === accountId);
        if (f.status === "failed" && f.problem === "nameTaken" && acc && provider) {
          const owner = namespace ?? acc.username;
          const existing = repoUrl(provider.baseUrl, owner, name.trim());
          setProblem({
            text: t("publish.error.nameTaken", { name: `${owner}/${name.trim()}` }),
            action: {
              label: t("publish.useExisting"),
              run: () => {
                setProblem(null);
                setUrl(existing);
                setUrlAccountChoice(acc.id);
                setMode("url");
              },
            },
          });
          return;
        }
        setProblem(
          describeForkFailure(
            t,
            f,
            accountId ?? "",
            acc ? accountLabel(acc) : (accountId ?? ""),
            i18n.language,
          ),
        );
      }
    }
  };

  const submit = async () => {
    if (!canSubmit) return;
    setProblem(null);
    const target = {
      repoId,
      branch: canPush && pushAfter ? branch : null,
      remoteName,
    };
    if (mode === "new" && account) {
      const step = await publishNewRepo(
        { ...target, accountId: account.id },
        {
          name: name.trim(),
          namespace,
          description: description.trim() || null,
          private: isPrivate,
        },
        protocol,
        setPhase,
      );
      setPhase(null);
      finish(step, account.id);
    } else {
      const step = await publishToUrl({ ...target, accountId: urlAccountId }, url, setPhase);
      setPhase(null);
      finish(step, urlAccountId);
    }
  };

  const stepState = (s: "commit" | "publish" | "push"): "done" | "current" | "todo" => {
    if (!steps) return "todo";
    if (s === "publish" && result) return "done";
    if (s === "push" && result?.pushed) return "done";
    if (steps.done[s]) return "done";
    if (s === "publish" && !result) return "current";
    if (s === "commit") return "current";
    return "todo";
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !running) onClose();
      }}
    >
      <DialogContent className="sm:max-w-lg" onEscapeKeyDown={(e) => running && e.preventDefault()}>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <CloudUpload className="size-4" />
            {t("publish.title")}
          </DialogTitle>
          <DialogDescription>
            {t("publish.description", { name: tab?.name ?? "" })}
          </DialogDescription>
        </DialogHeader>

        <ol className="flex items-center gap-2 text-[11px]" aria-label={t("publish.stepsLabel")}>
          {(["commit", "publish", "push"] as const).map((s, i) => (
            <li key={s} className="flex min-w-0 items-center gap-1.5">
              {i > 0 && <span className="h-px w-4 shrink-0 bg-border" aria-hidden />}
              <StepDot n={i + 1} state={stepState(s)} />
              <span className="truncate">
                {t(`publish.step.${s}`, { branch: branch ?? "main" })}
              </span>
            </li>
          ))}
        </ol>

        {refs?.head.unborn && !result && (
          <p
            className="rounded-md border border-warning/40 bg-warning/10 p-2 text-[11px] text-warning"
            role="note"
          >
            {t("publish.noCommitYet")}
          </p>
        )}

        <div
          role="radiogroup"
          aria-label={t("publish.modeLabel")}
          className="grid grid-cols-2 gap-1 rounded-md bg-muted p-0.5 text-xs"
        >
          {(["new", "url"] as const).map((m) => (
            <button
              key={m}
              type="button"
              role="radio"
              aria-checked={mode === m}
              disabled={locked}
              onClick={() => {
                setMode(m);
                setProblem(null);
              }}
              className={cn(
                "rounded px-2 py-1 font-medium",
                mode === m ? "bg-background shadow-sm" : "text-muted-foreground",
              )}
            >
              {t(`publish.mode.${m}`)}
            </button>
          ))}
        </div>

        <form
          className="grid grid-cols-[7rem_1fr] items-center gap-x-3 gap-y-2 text-xs"
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          {mode === "new" ? (
            <>
              <span className="font-semibold">{t("publish.account")}</span>
              {withToken.length === 0 ? (
                <div className="space-y-1">
                  <p className="text-warning">{t("publish.noAccount")}</p>
                  <Button
                    type="button"
                    size="xs"
                    variant="outline"
                    onClick={() => useAccountDialogs.getState().openManager({ add: true })}
                  >
                    <UserPlus />
                    {t("publish.addAccount")}
                  </Button>
                </div>
              ) : (
                <Select
                  value={account?.id ?? ""}
                  onValueChange={(v) => {
                    setAccountChoice(v);
                    setNamespace(null);
                  }}
                  disabled={locked}
                >
                  <SelectTrigger className="w-full text-xs" aria-label={t("publish.account")}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {withToken.map((a) => {
                      const p = providers?.find((x) => x.id === a.providerId);
                      return (
                        <SelectItem key={a.id} value={a.id} className="text-xs">
                          <span
                            aria-hidden
                            className="mr-1 inline-block size-2 rounded-full"
                            style={{ backgroundColor: a.color }}
                          />
                          {accountLabel(a)}{" "}
                          <span className="text-muted-foreground">
                            @{a.username}
                            {p ? ` · ${p.name}` : ""}
                          </span>
                        </SelectItem>
                      );
                    })}
                  </SelectContent>
                </Select>
              )}

              {account && (
                <>
                  <span className="font-semibold">{t("publish.namespace")}</span>
                  <NamespaceSelect
                    accountId={account.id}
                    value={namespace}
                    onChange={setNamespace}
                    disabled={locked}
                  />
                </>
              )}

              <label htmlFor="publish-name" className="font-semibold">
                {t("publish.name")}
              </label>
              <div className="space-y-0.5">
                <input
                  id="publish-name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  aria-invalid={nameInvalid}
                  disabled={locked}
                  className={cn(inputClass, "font-mono")}
                />
                {nameInvalid && (
                  <p className="text-[11px] text-danger">{t("publish.nameInvalid")}</p>
                )}
              </div>

              <label htmlFor="publish-description" className="font-semibold">
                {t("publish.descriptionLabel")}
              </label>
              <input
                id="publish-description"
                value={description}
                placeholder={t("publish.descriptionPlaceholder")}
                onChange={(e) => setDescription(e.target.value)}
                disabled={locked}
                className={inputClass}
              />

              <span className="font-semibold">{t("publish.visibility")}</span>
              <fieldset className="flex items-center gap-3" disabled={locked}>
                <label className="flex items-center gap-1.5">
                  <input
                    type="radio"
                    name="publish-visibility"
                    checked={isPrivate}
                    onChange={() => setIsPrivate(true)}
                  />
                  {t("publish.private")}
                </label>
                <label className="flex items-center gap-1.5">
                  <input
                    type="radio"
                    name="publish-visibility"
                    checked={!isPrivate}
                    onChange={() => setIsPrivate(false)}
                  />
                  {t("publish.public")}
                </label>
              </fieldset>

              <span className="font-semibold">{t("publish.protocol")}</span>
              <fieldset className="flex items-center gap-3" disabled={locked}>
                {(["https", "ssh"] as const).map((p) => (
                  <label key={p} className="flex items-center gap-1.5">
                    <input
                      type="radio"
                      name="publish-protocol"
                      checked={protocol === p}
                      onChange={() => setProtocol(p)}
                    />
                    {p.toUpperCase()}
                  </label>
                ))}
              </fieldset>
            </>
          ) : (
            <>
              <label htmlFor="publish-url" className="font-semibold">
                {t("publish.url")}
              </label>
              <div className="space-y-0.5">
                <input
                  id="publish-url"
                  value={url}
                  placeholder="https://github.com/you/project.git"
                  onChange={(e) => setUrl(e.target.value)}
                  aria-invalid={url !== "" && urlInvalid}
                  disabled={locked}
                  className={cn(inputClass, "font-mono")}
                />
                <p className="text-[11px] text-muted-foreground">{t("publish.urlHelp")}</p>
              </div>

              <span className="font-semibold">{t("publish.account")}</span>
              <Select
                value={urlAccountId ?? AUTO}
                onValueChange={setUrlAccountChoice}
                disabled={locked}
              >
                <SelectTrigger className="w-full text-xs" aria-label={t("publish.account")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={AUTO} className="text-xs">
                    {t("publish.accountAuto")}
                  </SelectItem>
                  {urlAccounts.map((a) => (
                    <SelectItem key={a.id} value={a.id} className="text-xs">
                      <span
                        aria-hidden
                        className="mr-1 inline-block size-2 rounded-full"
                        style={{ backgroundColor: a.color }}
                      />
                      {accountLabel(a)} <span className="text-muted-foreground">@{a.username}</span>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </>
          )}

          <label htmlFor="publish-remote" className="font-semibold">
            {t("publish.remoteName")}
          </label>
          <input
            id="publish-remote"
            value={remoteName}
            onChange={(e) => setRemoteEdit(e.target.value)}
            aria-invalid={remoteInvalid}
            disabled={locked}
            className={cn(inputClass, "w-40 font-mono")}
          />

          <span />
          <label className={cn("flex items-center gap-1.5", !canPush && "opacity-50")}>
            <input
              type="checkbox"
              checked={canPush && pushAfter}
              disabled={!canPush || locked}
              onChange={(e) => setPushAfter(e.target.checked)}
            />
            {t("publish.pushAfter", { branch: branch ?? "main" })}
          </label>
          {/* Enter in a field submits. */}
          <button type="submit" hidden />
        </form>

        {phase && (
          <p className="flex items-center gap-2 text-xs text-muted-foreground" role="status">
            <LoaderCircle className="size-3.5 animate-spin" />
            {t(`publish.phase.${phase}`, { remote: remoteName, branch: branch ?? "" })}
          </p>
        )}
        {problem && (
          <div
            className="rounded-md border border-danger/40 bg-danger/10 p-2 text-xs text-danger"
            role="alert"
          >
            {problem.text}{" "}
            {problem.action && (
              <button type="button" className="underline" onClick={problem.action.run}>
                {problem.action.label}
              </button>
            )}
          </div>
        )}
        {result && (
          <div
            className={cn(
              "rounded-md border p-2 text-xs",
              result.pushFailed
                ? "border-warning/40 bg-warning/10"
                : "border-success/40 bg-success/10",
            )}
            role="status"
          >
            {result.pushFailed
              ? t("publish.donePushFailed", { remote: result.change.remote })
              : result.pushed
                ? t("publish.done", {
                    name: result.repo?.fullName ?? result.change.url,
                    remote: result.change.remote,
                    branch: branch ?? "",
                  })
                : t("publish.doneNoPush", {
                    name: result.repo?.fullName ?? result.change.url,
                    remote: result.change.remote,
                  })}{" "}
            {result.repo && /^https:\/\//i.test(result.repo.webUrl) && (
              <button
                type="button"
                className="text-primary underline"
                onClick={() => void openExternal(result.repo!.webUrl)}
              >
                {t("publish.openOnWeb")}
              </button>
            )}
          </div>
        )}

        <DialogFooter>
          <Button variant="outline" size="sm" disabled={running} onClick={onClose}>
            {result ? t("publish.close") : t("common.cancel")}
          </Button>
          {!result && (
            <Button size="sm" disabled={!canSubmit} onClick={() => void submit()}>
              <CloudUpload />
              {mode === "new"
                ? canPush && pushAfter
                  ? t("publish.submitNewPush")
                  : t("publish.submitNew")
                : canPush && pushAfter
                  ? t("publish.submitUrlPush")
                  : t("publish.submitUrl")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
