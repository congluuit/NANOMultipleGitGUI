import { useMemo, useState } from "react";
import { GitFork, X } from "lucide-react";
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
import { useAccounts, useProviders } from "@/features/accounts/queries";
import { accountLabel } from "@/features/accounts/switcher/model";
import type { DescribedProblem } from "@/features/ci/errors";
import { openExternal } from "@/features/accounts/manager/openExternal";
import { useRepoRefs } from "@/features/repos/queries";
import type { ForkAttachMode, ForkRepo } from "@/ipc";
import { cn } from "@/lib/utils";
import { useUi } from "@/stores/ui";
import { cancelFork, forkOpenRepo } from "./actions";
import { ForkPhaseLine } from "./CloneForkOption";
import {
  accountsOnHost,
  describeForkFailure,
  freeRemoteName,
  validRemoteName,
  type ForkPhase,
} from "./model";
import { NamespaceSelect } from "./NamespaceSelect";
import { useForkSourceRemotes } from "./queries";
import { useForkDialog } from "./store";

const inputClass =
  "h-8 w-48 rounded-md border bg-background px-2 font-mono text-xs focus:border-primary focus:outline-none aria-invalid:border-danger";

/** Mounted once (App.tsx); opens through `openForkDialog(repoId, remote?)`. */
export function ForkDialogHost() {
  const preset = useForkDialog((s) => s.preset);
  const close = useForkDialog((s) => s.close);
  if (!preset) return null;
  return (
    <ForkDialog
      key={`fork-${preset.repoId}-${preset.remote ?? ""}`}
      repoId={preset.repoId}
      presetRemote={preset.remote}
      onClose={close}
    />
  );
}

/**
 * "Fork this repository…": fork the project of a remote with an account (and namespace), then
 * add the fork as a remote bound to that account (or swap it with `origin`) and fetch it.
 */
export function ForkDialog({
  repoId,
  presetRemote,
  onClose,
}: {
  repoId: number;
  presetRemote: string | null;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  const accounts = useAccounts().data;
  const providers = useProviders().data;
  const refs = useRepoRefs(repoId).data;
  const hosted = useForkSourceRemotes(repoId);
  const remotes = useMemo(() => hosted.data ?? [], [hosted.data]);
  const [remoteChoice, setRemoteChoice] = useState<string | null>(presetRemote);
  const source =
    remotes.find((r) => r.remote === remoteChoice) ??
    remotes.find((r) => r.remote === "upstream") ??
    remotes[0] ??
    null;
  const candidates = useMemo(
    () => accountsOnHost(accounts ?? [], providers, source?.host ?? null),
    [accounts, providers, source?.host],
  );
  const [accountChoice, setAccountChoice] = useState<string | null>(null);
  const accountId =
    accountChoice ??
    (candidates.some((a) => a.id === source?.accountId) ? source?.accountId : null) ??
    candidates[0]?.id ??
    null;
  const account = candidates.find((a) => a.id === accountId);
  const [namespace, setNamespace] = useState<string | null>(null);
  const existingNames = useMemo(() => refs?.remotes.map((r) => r.name) ?? [], [refs]);
  const [modeKind, setModeKind] = useState<ForkAttachMode["kind"]>("addRemote");
  const [nameEdit, setNameEdit] = useState<string | null>(null);
  const remoteName = nameEdit ?? freeRemoteName(existingNames);
  const canSwap = source?.remote === "origin" && !existingNames.includes("upstream");
  const mode: ForkAttachMode =
    modeKind === "swapOrigin" && canSwap
      ? { kind: "swapOrigin" }
      : { kind: "addRemote", name: remoteName.trim() };
  const nameInvalid =
    mode.kind === "addRemote" && (!validRemoteName(mode.name) || existingNames.includes(mode.name));

  const [phase, setPhase] = useState<ForkPhase | null>(null);
  const [opId, setOpId] = useState<string | null>(null);
  const [problem, setProblem] = useState<DescribedProblem | null>(null);
  const [done, setDone] = useState<ForkRepo | null>(null);
  const running = phase !== null;
  const ownRepo =
    !!account &&
    !!source &&
    source.fullName.split("/")[0]?.toLowerCase() === account.username.toLowerCase();

  const submit = async () => {
    if (!source || !account || running || nameInvalid) return;
    setProblem(null);
    const step = await forkOpenRepo(
      {
        repoId,
        accountId: account.id,
        sourceRemote: source.remote,
        source: source.fullName,
        options: { namespace },
        mode,
      },
      setPhase,
      setOpId,
    );
    setPhase(null);
    setOpId(null);
    switch (step.status) {
      case "done":
        setDone(step.fork);
        useUi.getState().showNotice({
          kind: "info",
          message: t("forks.attached", { name: step.fork.fullName, remote: step.change.remote }),
        });
        return;
      case "forkFailed":
        setProblem(
          describeForkFailure(t, step.failure, account.id, accountLabel(account), i18n.language),
        );
        return;
      case "notReady":
        setProblem({ text: t("forks.error.notReady", { name: step.fork.fullName }), action: null });
        return;
      case "cancelled":
        setProblem({ text: t("forks.cancelled"), action: null });
        return;
      case "failed":
        setProblem({ text: step.message, action: null });
        return;
    }
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
            <GitFork className="size-4" />
            {t("forks.title")}
          </DialogTitle>
          <DialogDescription>{t("forks.description")}</DialogDescription>
        </DialogHeader>

        {hosted.isSuccess && remotes.length === 0 ? (
          <p className="text-xs text-muted-foreground">{t("forks.noHostedRemote")}</p>
        ) : (
          <div className="grid grid-cols-[7rem_1fr] items-center gap-x-3 gap-y-2 text-xs">
            <span className="font-semibold">{t("forks.source")}</span>
            <Select
              value={source?.remote ?? ""}
              onValueChange={(v) => {
                setRemoteChoice(v);
                setAccountChoice(null);
                setNamespace(null);
              }}
              disabled={running || done !== null}
            >
              <SelectTrigger className="w-full text-xs" aria-label={t("forks.source")}>
                <SelectValue placeholder={t("forks.loading")} />
              </SelectTrigger>
              <SelectContent>
                {remotes.map((r) => (
                  <SelectItem key={r.remote} value={r.remote} className="text-xs">
                    {r.remote} · {r.fullName}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <span className="font-semibold">{t("forks.asAccount")}</span>
            {candidates.length === 0 ? (
              <span className="text-warning">
                {t("forks.noAccountOnHost", { host: source?.host ?? "?" })}
              </span>
            ) : (
              <Select
                value={accountId ?? ""}
                onValueChange={(v) => {
                  setAccountChoice(v);
                  setNamespace(null);
                }}
                disabled={running || done !== null}
              >
                <SelectTrigger className="w-full text-xs" aria-label={t("forks.asAccount")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {candidates.map((a) => (
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
            )}

            {account && (
              <>
                <span className="font-semibold">{t("forks.namespace")}</span>
                <NamespaceSelect
                  accountId={account.id}
                  value={namespace}
                  onChange={setNamespace}
                  disabled={running || done !== null}
                />
              </>
            )}

            <span className="self-start pt-1 font-semibold">{t("forks.remotes")}</span>
            <fieldset
              className="space-y-1"
              aria-label={t("forks.remotes")}
              disabled={running || done !== null}
            >
              <label className="flex items-center gap-1.5">
                <input
                  type="radio"
                  name="fork-mode"
                  checked={mode.kind === "addRemote"}
                  onChange={() => setModeKind("addRemote")}
                />
                {t("forks.mode.add")}
                <input
                  value={remoteName}
                  onChange={(e) => {
                    setNameEdit(e.target.value);
                    setModeKind("addRemote");
                  }}
                  aria-label={t("forks.remoteName")}
                  aria-invalid={nameInvalid}
                  className={cn(inputClass, "ml-1 h-6 w-32")}
                />
              </label>
              <label className={cn("flex items-center gap-1.5", !canSwap && "opacity-50")}>
                <input
                  type="radio"
                  name="fork-mode"
                  checked={mode.kind === "swapOrigin"}
                  disabled={!canSwap}
                  onChange={() => setModeKind("swapOrigin")}
                />
                {t("forks.mode.swap")}
              </label>
              <p className="text-[11px] text-muted-foreground">
                {mode.kind === "swapOrigin" ? t("forks.mode.swapHelp") : t("forks.mode.addHelp")}
              </p>
            </fieldset>
          </div>
        )}

        {ownRepo && !done && (
          <p className="text-[11px] text-warning" role="note">
            {t("forks.ownRepoWarning")}
          </p>
        )}
        <ForkPhaseLine phase={phase} />
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
        {done && (
          <div
            className="rounded-md border border-success/40 bg-success/10 p-2 text-xs"
            role="status"
          >
            {done.existed
              ? t("forks.doneExisting", { name: done.fullName })
              : t("forks.done", { name: done.fullName })}{" "}
            {/^https:\/\//i.test(done.webUrl) && (
              <button
                type="button"
                className="text-primary underline"
                onClick={() => void openExternal(done.webUrl)}
              >
                {t("forks.openOnWeb")}
              </button>
            )}
          </div>
        )}

        <DialogFooter>
          {running ? (
            <Button
              variant="outline"
              size="sm"
              disabled={phase !== "waiting" || !opId}
              onClick={() => opId && void cancelFork(opId)}
            >
              <X />
              {t("forks.cancel")}
            </Button>
          ) : (
            <Button variant="outline" size="sm" onClick={onClose}>
              {done ? t("forks.close") : t("common.cancel")}
            </Button>
          )}
          {!done && (
            <Button
              size="sm"
              disabled={!source || !account || running || nameInvalid}
              onClick={() => void submit()}
            >
              <GitFork />
              {account
                ? t("forks.submitAs", { account: accountLabel(account) })
                : t("forks.submit")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
