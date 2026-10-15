import { useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  Check,
  Copy,
  ExternalLink,
  KeyRound,
  LoaderCircle,
  MonitorSmartphone,
  Globe,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  commands,
  errorMessage,
  type Account,
  type DeviceStart,
  type PkceStart,
  type Provider,
  type ProviderKind,
} from "@/ipc";
import { cn } from "@/lib/utils";
import { AccountAvatar } from "../AccountAvatar";
import { authMethodText } from "../cli-sync/labels";
import { useIdentities } from "../queries";
import { describeAuthError, type DescribedError } from "./errors";
import { copyText, openExternal } from "./openExternal";
import { refreshAccounts, useAuthConfig } from "./queries";
import { Badge, Callout, Field, TextInput } from "./ui";
import {
  PROVIDER_CHOICES,
  countdown,
  findProvider,
  instanceName,
  isSelfHosted,
  methodsFor,
  normalizeBaseUrl,
  validateBaseUrl,
  validateToken,
  type AuthMethodChoice,
  type TokenProblem,
  type UrlProblem,
  type WizardStep,
} from "./wizard";

interface Props {
  providers: readonly Provider[];
  /** Re-authenticate this account: the provider is fixed, PAT replaces its token. */
  reauth?: Account | null;
  onDone: (account: Account) => void;
  onCancel: () => void;
  /** Jump to the Providers tab (to enter an OAuth client id). */
  onEditProvider?: (providerId: string) => void;
}

const METHOD_ICONS = { device: MonitorSmartphone, pkce: Globe, pat: KeyRound } as const;

/** Add-account wizard: provider → method → device code / browser / PAT → done. */
export function AddAccountWizard({ providers, reauth, onDone, onCancel, onEditProvider }: Props) {
  const { t } = useTranslation();
  const reauthProvider = reauth
    ? (providers.find((p) => p.id === reauth.providerId) ?? null)
    : null;
  const [step, setStep] = useState<WizardStep>(reauthProvider ? "method" : "provider");
  const [kind, setKind] = useState<ProviderKind>(reauthProvider?.kind ?? "github");
  const [baseUrl, setBaseUrl] = useState("");
  const [urlError, setUrlError] = useState<UrlProblem | null>(null);
  const [provider, setProvider] = useState<Provider | null>(reauthProvider);
  const [error, setError] = useState<DescribedError | null>(null);
  const [busy, setBusy] = useState(false);
  const [device, setDevice] = useState<DeviceStart | null>(null);
  const [pkce, setPkce] = useState<PkceStart | null>(null);
  const [token, setToken] = useState("");
  const [tokenError, setTokenError] = useState<TokenProblem | null>(null);
  const [label, setLabel] = useState("");
  const [result, setResult] = useState<Account | null>(null);
  const config = useAuthConfig(provider?.id ?? null);

  // The running device/PKCE flow; cancelled when the wizard goes away.
  const flowRef = useRef<string | null>(null);
  const generation = useRef(0);
  useEffect(
    () => () => {
      if (flowRef.current) void commands.authCancel(flowRef.current);
    },
    [],
  );

  const cancelFlow = () => {
    generation.current += 1;
    if (flowRef.current) void commands.authCancel(flowRef.current);
    flowRef.current = null;
  };

  const finish = (account: Account) => {
    refreshAccounts();
    setResult(account);
    setStep("done");
  };

  const fail = (e: unknown) => {
    const described = describeAuthError(t, e);
    if (!described.cancelled) setError(described);
  };

  const confirmProvider = async () => {
    setError(null);
    if (isSelfHosted(kind)) {
      const problem = validateBaseUrl(baseUrl);
      setUrlError(problem);
      if (problem) return;
    }
    let p = findProvider(providers, kind, baseUrl);
    if (!p) {
      setBusy(true);
      try {
        p = await commands.providerSave({
          kind,
          name: instanceName(kind, baseUrl),
          baseUrl: normalizeBaseUrl(baseUrl),
        });
        refreshAccounts();
      } catch (e) {
        setError({ message: errorMessage(e) });
        return;
      } finally {
        setBusy(false);
      }
    }
    setProvider(p);
    setStep("method");
  };

  const startDevice = async (p: Provider) => {
    const gen = ++generation.current;
    setStep("device");
    setError(null);
    setDevice(null);
    try {
      const start = await commands.authDeviceStart(p.id, null);
      if (gen !== generation.current) {
        void commands.authCancel(start.flowId);
        return;
      }
      flowRef.current = start.flowId;
      setDevice(start);
      void copyText(start.prompt.userCode);
      void openExternal(start.prompt.verificationUriComplete ?? start.prompt.verificationUri);
      const account = await commands.authDeviceWait(start.flowId);
      if (gen !== generation.current) return;
      flowRef.current = null;
      finish(account);
    } catch (e) {
      if (gen !== generation.current) return;
      flowRef.current = null;
      fail(e);
    }
  };

  const startPkce = async (p: Provider) => {
    const gen = ++generation.current;
    setStep("pkce");
    setError(null);
    setPkce(null);
    try {
      const start = await commands.authPkceStart(p.id, null);
      if (gen !== generation.current) {
        void commands.authCancel(start.flowId);
        return;
      }
      flowRef.current = start.flowId;
      setPkce(start);
      void openExternal(start.authorizeUrl);
      const account = await commands.authPkceWait(start.flowId);
      if (gen !== generation.current) return;
      flowRef.current = null;
      finish(account);
    } catch (e) {
      if (gen !== generation.current) return;
      flowRef.current = null;
      fail(e);
    }
  };

  const chooseMethod = (method: AuthMethodChoice) => {
    if (!provider) return;
    setError(null);
    if (method === "device") void startDevice(provider);
    else if (method === "pkce") void startPkce(provider);
    else setStep("pat");
  };

  const submitPat = async () => {
    if (!provider) return;
    const problem = validateToken(token);
    setTokenError(problem);
    if (problem) return;
    setBusy(true);
    setError(null);
    try {
      const account = reauth
        ? await commands.authUpdateToken(reauth.id, token)
        : await commands.authAddPat(provider.id, token, label.trim() || null);
      setToken("");
      finish(account);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  const back = () => {
    cancelFlow();
    setError(null);
    if (step === "method" && !reauthProvider) setStep("provider");
    else if (step === "method") onCancel();
    else setStep("method");
  };

  const usePat = () => {
    cancelFlow();
    setError(null);
    setStep("pat");
  };

  const title = reauth
    ? t("auth.wizard.reauthTitle", { name: reauth.label ?? reauth.username })
    : t("auth.wizard.title");

  return (
    <section aria-label={title} className="grid gap-4">
      <header className="flex items-center gap-2">
        {step !== "provider" && step !== "done" && (
          <Button variant="ghost" size="icon-sm" aria-label={t("auth.wizard.back")} onClick={back}>
            <ArrowLeft />
          </Button>
        )}
        <div className="grid gap-0.5">
          <h3 className="text-sm font-semibold">{title}</h3>
          {provider && step !== "provider" && (
            <p className="text-[11px] text-muted-foreground">
              {provider.name} · {provider.baseUrl}
            </p>
          )}
        </div>
      </header>

      {step === "provider" && (
        <form
          className="grid gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            if (!busy) void confirmProvider();
          }}
        >
          <div
            role="radiogroup"
            aria-label={t("auth.wizard.chooseProvider")}
            className="grid grid-cols-2 gap-2"
          >
            {PROVIDER_CHOICES.map((k) => (
              <button
                key={k}
                type="button"
                role="radio"
                aria-checked={kind === k}
                onClick={() => {
                  setKind(k);
                  setUrlError(null);
                }}
                className={cn(
                  "grid gap-0.5 rounded-md border p-3 text-left text-xs hover:bg-accent",
                  kind === k && "border-primary ring-1 ring-primary",
                )}
              >
                <span className="font-medium">{t(`auth.kind.${k}`)}</span>
                <span className="text-[11px] text-muted-foreground">{t(`auth.kindHint.${k}`)}</span>
              </button>
            ))}
          </div>
          {isSelfHosted(kind) && (
            <Field
              label={t("auth.wizard.baseUrl")}
              hint={t("auth.wizard.baseUrlHint")}
              error={urlError && t(`auth.urlError.${urlError}`)}
            >
              <TextInput
                value={baseUrl}
                onChange={(e) => {
                  setBaseUrl(e.target.value);
                  setUrlError(null);
                }}
                placeholder={
                  kind === "gitlabSelfHosted" ? "https://gitlab.acme.vn" : "https://github.acme.vn"
                }
                aria-invalid={urlError !== null}
                autoFocus
              />
            </Field>
          )}
          {error && <Callout tone="danger">{error.message}</Callout>}
          <div className="flex justify-end gap-2">
            <Button type="button" variant="outline" size="sm" onClick={onCancel}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={busy}>
              {busy && <LoaderCircle className="animate-spin" />}
              {t("auth.wizard.next")}
            </Button>
          </div>
        </form>
      )}

      {step === "method" && provider && (
        <div className="grid gap-3">
          {config.data && !config.data.hasClientId && (
            <Callout tone="warn">
              <p>{t("auth.wizard.noClientId")}</p>
              {onEditProvider && (
                <Button
                  variant="link"
                  size="xs"
                  className="h-auto px-0"
                  onClick={() => onEditProvider(provider.id)}
                >
                  {t("auth.wizard.enterClientId")}
                </Button>
              )}
            </Callout>
          )}
          <ul aria-label={t("auth.wizard.chooseMethod")} className="grid gap-2">
            {methodsFor(provider.kind).map((m) => {
              const Icon = METHOD_ICONS[m];
              const needsClient = m !== "pat";
              const disabled = needsClient && config.data?.hasClientId === false;
              return (
                <li key={m} className="grid">
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => chooseMethod(m)}
                    className="flex items-start gap-3 rounded-md border p-3 text-left text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                    <span className="grid gap-0.5">
                      <span className="font-medium">{t(`auth.method.${m}`)}</span>
                      <span className="text-[11px] text-muted-foreground">
                        {t(`auth.methodHint.${m}`)}
                      </span>
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      )}

      {step === "device" && (
        <div className="grid gap-3">
          {error ? (
            <FlowError
              error={error}
              onRetry={() => provider && void startDevice(provider)}
              onUsePat={usePat}
            />
          ) : !device ? (
            <Waiting text={t("auth.device.requesting")} />
          ) : (
            <DeviceCode start={device} />
          )}
          <div className="flex justify-end">
            <Button variant="outline" size="sm" onClick={back}>
              {t("common.cancel")}
            </Button>
          </div>
        </div>
      )}

      {step === "pkce" && (
        <div className="grid gap-3">
          {error ? (
            <FlowError
              error={error}
              onRetry={() => provider && void startPkce(provider)}
              onUsePat={usePat}
            />
          ) : !pkce ? (
            <Waiting text={t("auth.pkce.starting")} />
          ) : (
            <div className="grid gap-3">
              <p className="text-xs">{t("auth.pkce.opened")}</p>
              <div className="flex flex-wrap gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => void openExternal(pkce.authorizeUrl)}
                >
                  <ExternalLink />
                  {t("auth.pkce.openAgain")}
                </Button>
              </div>
              <Callout>
                <p>{t("auth.pkce.redirectHint")}</p>
                <code className="mt-1 block font-mono text-[11px] select-all">
                  {pkce.redirectUri}
                </code>
              </Callout>
              <Waiting text={t("auth.pkce.waiting")} />
            </div>
          )}
          <div className="flex justify-end">
            <Button variant="outline" size="sm" onClick={back}>
              {t("common.cancel")}
            </Button>
          </div>
        </div>
      )}

      {step === "pat" && provider && (
        <form
          className="grid gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            if (!busy) void submitPat();
          }}
        >
          <Callout>
            <p>{t("auth.pat.scopesHint")}</p>
            <div className="mt-1.5 flex flex-wrap gap-1">
              {(config.data?.patScopes ?? []).map((s) => (
                <Badge key={s} className="font-mono">
                  {s}
                </Badge>
              ))}
            </div>
            {config.data && (
              <Button
                type="button"
                variant="link"
                size="xs"
                className="mt-1 h-auto px-0"
                onClick={() => void openExternal(config.data.patUrl)}
              >
                <ExternalLink />
                {t("auth.pat.createLink")}
              </Button>
            )}
            {(provider.kind === "github" || provider.kind === "githubEnterprise") && (
              <p className="mt-1 text-[11px] text-muted-foreground">{t("auth.pat.fineGrained")}</p>
            )}
          </Callout>
          <Field
            label={t("auth.pat.token")}
            error={tokenError && t(`auth.pat.error.${tokenError}`)}
          >
            <TextInput
              type="password"
              autoComplete="off"
              spellCheck={false}
              value={token}
              onChange={(e) => {
                setToken(e.target.value);
                setTokenError(null);
              }}
              aria-invalid={tokenError !== null}
              autoFocus
            />
          </Field>
          {!reauth && (
            <Field label={t("accounts.field.label")} hint={t("accounts.field.labelHint")}>
              <TextInput value={label} onChange={(e) => setLabel(e.target.value)} />
            </Field>
          )}
          {error && <Callout tone="danger">{error.message}</Callout>}
          <div className="flex justify-end gap-2">
            <Button type="button" variant="outline" size="sm" onClick={back}>
              {t("auth.wizard.back")}
            </Button>
            <Button type="submit" size="sm" disabled={busy}>
              {busy && <LoaderCircle className="animate-spin" />}
              {t("auth.pat.submit")}
            </Button>
          </div>
        </form>
      )}

      {step === "done" && result && (
        <div className="grid gap-3">
          <Callout>
            <span className="flex items-center gap-1.5 font-medium text-success">
              <Check className="size-4" />
              {t("auth.done.title")}
            </span>
          </Callout>
          <DoneSummary account={result} provider={provider} />
          <div className="flex justify-end">
            <Button size="sm" onClick={() => onDone(result)}>
              {t("auth.done.finish")}
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}

function Waiting({ text }: { text: string }) {
  return (
    <p role="status" className="flex items-center gap-2 text-xs text-muted-foreground">
      <LoaderCircle className="size-3.5 animate-spin" />
      {text}
    </p>
  );
}

function FlowError({
  error,
  onRetry,
  onUsePat,
}: {
  error: DescribedError;
  onRetry: () => void;
  onUsePat: () => void;
}) {
  const { t } = useTranslation();
  const suggestPat = Boolean(error.pat || error.deviceUnsupported || error.oauthRestricted);
  return (
    <div className="grid gap-2">
      <Callout tone="danger">{error.message}</Callout>
      <div className="flex flex-wrap gap-2">
        {!error.pat && (
          <Button size="sm" variant="outline" onClick={onRetry}>
            {t("auth.wizard.retry")}
          </Button>
        )}
        {suggestPat && (
          <Button size="sm" onClick={onUsePat}>
            <KeyRound />
            {t("auth.wizard.usePat")}
          </Button>
        )}
      </div>
    </div>
  );
}

/** Ticks every second (the state update happens in the timer callback, not during render). */
function useNow(): number | null {
  const [now, setNow] = useState<number | null>(null);
  useEffect(() => {
    const tick = () => setNow(Date.now());
    const first = window.setTimeout(tick, 0);
    const id = window.setInterval(tick, 1000);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(id);
    };
  }, []);
  return now;
}

export function DeviceCode({ start }: { start: DeviceStart }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const now = useNow();
  const left = now === null ? "" : countdown(start.prompt.expiresAt, now);
  const url = start.prompt.verificationUriComplete ?? start.prompt.verificationUri;
  return (
    <div className="grid gap-3">
      <p className="text-xs">
        {t("auth.device.instructions", { url: start.prompt.verificationUri })}
      </p>
      <div className="flex items-center justify-center gap-3 rounded-md border bg-muted/40 py-4">
        <code
          aria-label={t("auth.device.code")}
          className="font-mono text-3xl font-semibold tracking-[0.2em] select-all"
        >
          {start.prompt.userCode}
        </code>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("auth.device.copy")}
          onClick={() => void copyText(start.prompt.userCode).then(setCopied)}
        >
          {copied ? <Check /> : <Copy />}
        </Button>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          onClick={() => {
            void copyText(start.prompt.userCode).then(setCopied);
            void openExternal(url);
          }}
        >
          <ExternalLink />
          {t("auth.device.open")}
        </Button>
        <span className="text-[11px] text-muted-foreground">
          {left === null
            ? t("auth.device.expired")
            : left && t("auth.device.expiresIn", { time: left })}
        </span>
      </div>
      <Waiting text={t("auth.device.waiting")} />
    </div>
  );
}

function DoneSummary({ account, provider }: { account: Account; provider: Provider | null }) {
  const { t } = useTranslation();
  const identities = useIdentities(account.id);
  const identity = identities.data?.find((i) => i.isDefault) ?? identities.data?.[0];
  return (
    <div className="flex items-center gap-3 rounded-md border p-3">
      <AccountAvatar account={account} size={36} />
      <div className="grid min-w-0 gap-0.5 text-xs">
        <span className="font-medium">
          {account.displayName ?? account.username}{" "}
          <span className="text-muted-foreground">@{account.username}</span>
        </span>
        <span className="text-muted-foreground">
          {provider?.name} · {authMethodText(t, account, provider?.kind)}
        </span>
        {identity && (
          <span className="truncate">
            {t("auth.done.identity")}: {identity.name} &lt;{identity.email}&gt;
          </span>
        )}
      </div>
    </div>
  );
}
