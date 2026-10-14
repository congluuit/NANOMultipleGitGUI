import { useState } from "react";
import { ExternalLink, LoaderCircle, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { commands, errorMessage, type Provider } from "@/ipc";
import { cn } from "@/lib/utils";
import { openExternal } from "./openExternal";
import { refreshAccounts, useAuthConfig } from "./queries";
import { Badge, Callout, Field, TextInput } from "./ui";
import { isGitlab } from "./wizard";

/** Provider instances: OAuth client id, loopback port (GitLab PKCE), large-file thresholds. */
export function ProvidersTab({
  providers,
  focusId,
}: {
  providers: readonly Provider[];
  focusId: string | null;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState<string | null>(focusId);
  return (
    <div className="grid gap-2">
      <p className="text-[11px] text-muted-foreground">{t("accounts.providers.hint")}</p>
      <ul className="grid gap-2">
        {providers.map((p) => (
          <li key={p.id} className="rounded-md border">
            <button
              type="button"
              aria-expanded={open === p.id}
              onClick={() => setOpen((cur) => (cur === p.id ? null : p.id))}
              className="flex w-full items-center gap-2 p-2.5 text-left text-xs"
            >
              <span className="font-medium">{p.name}</span>
              <span className="truncate text-muted-foreground">{p.baseUrl}</span>
              {p.builtin && <Badge>{t("accounts.providers.builtin")}</Badge>}
              {p.oauthClientId ? (
                <Badge tone="ok" className="ml-auto">
                  OAuth
                </Badge>
              ) : (
                <Badge className="ml-auto">{t("accounts.providers.patOnly")}</Badge>
              )}
            </button>
            {open === p.id && <ProviderEditor provider={p} />}
          </li>
        ))}
      </ul>
    </div>
  );
}

function ProviderEditor({ provider }: { provider: Provider }) {
  const { t } = useTranslation();
  const config = useAuthConfig(provider.id);
  const [name, setName] = useState(provider.name);
  const [clientId, setClientId] = useState(provider.oauthClientId ?? "");
  const [port, setPort] = useState<string | null>(null);
  const [warn, setWarn] = useState(String(provider.largeFileWarnMb));
  const [block, setBlock] = useState(
    provider.largeFileBlockMb === null ? "" : String(provider.largeFileBlockMb),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const gitlab = isGitlab(provider.kind);
  const portValue = port ?? String(config.data?.loopbackPort ?? "");

  const save = async () => {
    setError(null);
    setSaved(false);
    const warnMb = Number(warn);
    const blockMb = block.trim() ? Number(block) : null;
    const portNum = Number(portValue);
    if (!Number.isInteger(warnMb) || warnMb <= 0) {
      setError(t("accounts.providers.error.warn"));
      return;
    }
    if (blockMb !== null && (!Number.isInteger(blockMb) || blockMb < warnMb)) {
      setError(t("accounts.providers.error.block"));
      return;
    }
    if (gitlab && (!Number.isInteger(portNum) || portNum < 1024 || portNum > 65535)) {
      setError(t("accounts.providers.error.port"));
      return;
    }
    setBusy(true);
    try {
      await commands.providerSave({
        id: provider.id,
        kind: provider.kind,
        name: name.trim() || provider.name,
        baseUrl: provider.baseUrl,
        apiUrl: provider.apiUrl,
        oauthClientId: clientId.trim() || null,
        largeFileWarnMb: warnMb,
        largeFileBlockMb: blockMb,
      });
      if (gitlab && portNum !== config.data?.loopbackPort) {
        await commands.authConfigSet(provider.id, portNum);
      }
      refreshAccounts();
      setSaved(true);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setError(null);
    try {
      await commands.providerDelete(provider.id);
      refreshAccounts();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <form
      className="grid gap-3 border-t p-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (!busy) void save();
      }}
    >
      <div className="grid grid-cols-2 gap-3">
        <Field label={t("accounts.providers.name")}>
          <TextInput value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label={t("accounts.providers.apiUrl")}>
          <TextInput value={provider.apiUrl} readOnly disabled />
        </Field>
      </div>
      <Field
        label={t("accounts.providers.clientId")}
        hint={
          config.data?.builtinClientId
            ? t("accounts.providers.clientIdBuiltin")
            : t("accounts.providers.clientIdHint")
        }
      >
        <TextInput
          value={clientId}
          spellCheck={false}
          placeholder={gitlab ? "Application ID" : "Iv1.…"}
          onChange={(e) => setClientId(e.target.value)}
        />
      </Field>
      {config.data && (
        <Button
          type="button"
          variant="link"
          size="xs"
          className="h-auto justify-self-start px-0"
          onClick={() => void openExternal(config.data.oauthAppsUrl)}
        >
          <ExternalLink />
          {t("accounts.providers.registerApp")}
        </Button>
      )}
      {gitlab && (
        <div className="grid grid-cols-[8rem_1fr] items-start gap-3">
          <Field label={t("accounts.providers.port")}>
            <TextInput
              inputMode="numeric"
              value={portValue}
              onChange={(e) => setPort(e.target.value.replace(/[^0-9]/g, ""))}
            />
          </Field>
          <Callout className="text-[11px]">
            <p>{t("accounts.providers.redirectHint")}</p>
            <code className="mt-1 block font-mono select-all">
              {`http://127.0.0.1:${portValue}/callback`}
            </code>
            <p className="mt-1 text-muted-foreground">{t("accounts.providers.scopesHint")}</p>
          </Callout>
        </div>
      )}
      {!gitlab && <Callout className="text-[11px]">{t("accounts.providers.deviceHint")}</Callout>}
      <div className="grid grid-cols-2 gap-3">
        <Field label={t("accounts.providers.warnMb")}>
          <TextInput inputMode="numeric" value={warn} onChange={(e) => setWarn(e.target.value)} />
        </Field>
        <Field label={t("accounts.providers.blockMb")} hint={t("accounts.providers.blockHint")}>
          <TextInput inputMode="numeric" value={block} onChange={(e) => setBlock(e.target.value)} />
        </Field>
      </div>
      {error && <Callout tone="danger">{error}</Callout>}
      <div className="flex items-center gap-2">
        {!provider.builtin && (
          <Button
            type="button"
            size="xs"
            variant="ghost"
            className="text-destructive"
            onClick={() => void remove()}
          >
            <Trash2 />
            {t("accounts.providers.delete")}
          </Button>
        )}
        <span className={cn("ml-auto text-[11px] text-success", !saved && "invisible")}>
          {t("accounts.saved")}
        </span>
        <Button type="submit" size="xs" disabled={busy}>
          {busy && <LoaderCircle className="animate-spin" />}
          {t("accounts.save")}
        </Button>
      </div>
    </form>
  );
}
