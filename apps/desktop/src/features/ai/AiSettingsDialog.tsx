import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import {
  CircleCheck,
  CircleX,
  ExternalLink,
  Eye,
  EyeOff,
  KeyRound,
  LoaderCircle,
  PlugZap,
  ShieldCheck,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
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
import { Switch } from "@/components/ui/switch";
import { openExternal } from "@/features/accounts/manager/openExternal";
import {
  type AiKeyStatus,
  type AiMessageLanguage,
  type AiMessageStyle,
  type AiProvider,
  type AiTestResult,
  commands,
  events,
} from "@/ipc";
import { cn } from "@/lib/utils";
import {
  CLOUD_MODELS,
  COMPATIBLE_PRESETS,
  type CloudProvider,
  type FullAiConfig,
  insecureRemote,
  isAiError,
  isCloudProvider,
  KEY_PAGES,
  newRequestId,
  PROVIDERS,
  sendsCodeAway,
  serviceName,
} from "./model";
import { OllamaPanel } from "./OllamaPanel";
import { saveAiConfig, setAiSettings, useAiConfig, useAiSettings } from "./queries";
import { useAiUi } from "./store";
import { aiErrorText } from "./suggest";
import "./palette";

/** Mounted once in `App`: the AI settings dialog + the model download progress listener. */
export function AiSettingsHost() {
  usePullProgressEvents();
  const open = useAiUi((s) => s.open);
  if (!open) return null;
  return <AiSettingsDialog />;
}

function usePullProgressEvents() {
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void events.aiPullProgress
      .listen(({ payload }) => {
        const pull = useAiUi.getState().pull;
        if (pull && pull.requestId === payload.requestId) {
          useAiUi.getState().setPull({ ...pull, progress: payload });
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
}

function AiSettingsDialog() {
  const { t } = useTranslation();
  const setOpen = useAiUi((s) => s.setOpen);
  const settings = useAiSettings();
  const config = useAiConfig();
  const keys = settings.data?.keys;

  const [saveError, setSaveError] = useState<string | null>(null);
  const save = (patch: Partial<FullAiConfig>) => {
    if (!config) return;
    setSaveError(null);
    void saveAiConfig({ ...config, ...patch }).catch((e) => setSaveError(aiErrorText(t, e)));
  };

  return (
    <Dialog open onOpenChange={setOpen}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("ai.settings.title")}</DialogTitle>
          <DialogDescription>{t("ai.settings.description")}</DialogDescription>
        </DialogHeader>

        {!config ? (
          <p className="text-sm text-muted-foreground">
            {isTauri() ? t("ai.loading") : t("common.backendBrowser")}
          </p>
        ) : (
          <div className="space-y-5 py-1">
            <div className="flex items-start gap-3 rounded-md border p-3 text-xs">
              <label htmlFor="ai-enabled" className="grid flex-1 cursor-pointer gap-0.5">
                <span className="font-medium">{t("ai.settings.enable")}</span>
                <span className="text-[11px] text-muted-foreground">
                  {t("ai.settings.enableHint")}
                </span>
              </label>
              <Switch
                id="ai-enabled"
                checked={config.enabled}
                onCheckedChange={(enabled) => save({ enabled })}
                className="mt-0.5"
              />
            </div>

            <section className="space-y-2">
              <div className="text-xs font-semibold">{t("ai.provider.title")}</div>
              <div className="grid gap-2 sm:grid-cols-2" role="radiogroup">
                {PROVIDERS.map((p) => (
                  <ProviderCard
                    key={`ai-provider-${p}`}
                    provider={p}
                    selected={config.provider === p}
                    hasKey={p !== "ollama" && keys ? keys[p] : false}
                    onSelect={() => save({ provider: p })}
                  />
                ))}
              </div>
            </section>

            {config.provider === "ollama" ? (
              <OllamaPanel config={config} save={save} />
            ) : (
              <CloudPanel
                key={`ai-cloud-${config.provider}`}
                provider={config.provider}
                config={config}
                keys={keys}
                save={save}
              />
            )}

            <OptionsSection config={config} save={save} />
            {saveError && <p className="text-xs text-danger">{saveError}</p>}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

function ProviderCard({
  provider,
  selected,
  hasKey,
  onSelect,
}: {
  provider: AiProvider;
  selected: boolean;
  hasKey: boolean;
  onSelect: () => void;
}) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onSelect}
      className={cn(
        "rounded-md border p-2.5 text-left text-xs transition-colors hover:bg-accent",
        selected && "border-primary ring-1 ring-primary",
        provider === "ollama" && "sm:col-span-2",
      )}
    >
      <div className="flex items-center gap-1.5 font-medium">
        {provider === "ollama" && <ShieldCheck className="size-3.5 text-success" />}
        {t(`ai.provider.${provider}.name`)}
        {hasKey && (
          <span className="ml-auto flex items-center gap-1 text-[10px] font-normal text-success">
            <KeyRound className="size-3" />
            {t("ai.key.saved")}
          </span>
        )}
      </div>
      <div className="mt-0.5 text-[11px] text-muted-foreground">
        {t(`ai.provider.${provider}.hint`)}
      </div>
    </button>
  );
}

/** Text field saved on blur / Enter (not on every key). */
function LazyInput({
  value,
  onCommit,
  ...props
}: {
  value: string;
  onCommit: (v: string) => void;
} & Omit<React.InputHTMLAttributes<HTMLInputElement>, "value" | "onChange">) {
  const [text, setText] = useState(value);
  const [seen, setSeen] = useState(value);
  if (value !== seen) {
    setSeen(value);
    setText(value);
  }
  const commit = () => {
    if (text.trim() !== value) onCommit(text.trim());
  };
  return (
    <input
      {...props}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
      }}
      className={cn(
        "h-8 w-full rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none",
        props.className,
      )}
    />
  );
}

function CloudPanel({
  provider,
  config,
  keys,
  save,
}: {
  provider: CloudProvider;
  config: FullAiConfig;
  keys: AiKeyStatus | undefined;
  save: (patch: Partial<FullAiConfig>) => void;
}) {
  const { t } = useTranslation();
  const hasKey = keys?.[provider] ?? false;
  const [key, setKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [keyBusy, setKeyBusy] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [testing, setTesting] = useState<string | null>(null);
  const [test, setTest] = useState<AiTestResult | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const model = config.models[provider];
  const listId = `ai-models-${provider}`;
  const suggestions = CLOUD_MODELS[provider];

  const saveKey = async () => {
    setKeyBusy(true);
    setKeyError(null);
    try {
      setAiSettings(await commands.aiKeySet(provider, key));
      setKey("");
      setShowKey(false);
    } catch (e) {
      setKeyError(aiErrorText(t, e));
    } finally {
      setKeyBusy(false);
    }
  };

  const deleteKey = async () => {
    setConfirmDelete(false);
    try {
      setAiSettings(await commands.aiKeyDelete(provider));
    } catch (e) {
      setKeyError(aiErrorText(t, e));
    }
  };

  const runTest = async () => {
    if (testing) {
      void commands.aiCancel(testing);
      setTesting(null);
      return;
    }
    const id = newRequestId("test");
    setTesting(id);
    setTest(null);
    setTestError(null);
    try {
      const r = await commands.aiTest(config, id);
      setTest(r);
    } catch (e) {
      if (!(isAiError(e) && e.kind === "cancelled")) setTestError(aiErrorText(t, e));
    } finally {
      setTesting((cur) => (cur === id ? null : cur));
    }
  };

  const modelChoices = [
    ...suggestions.map((s) => s.id),
    ...(test?.models ?? []).filter((m) => !suggestions.some((s) => s.id === m)),
  ];

  return (
    <section className="space-y-4">
      {sendsCodeAway(config) && (
        <p
          className="flex items-start gap-2 rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs"
          role="note"
        >
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-warning" />
          <span>{t("ai.cloudWarning", { service: serviceName(config) })}</span>
        </p>
      )}

      {provider === "compatible" && (
        <div className="space-y-1.5">
          <label htmlFor="ai-base-url" className="text-xs font-medium">
            {t("ai.baseUrl.label")}
          </label>
          <LazyInput
            id="ai-base-url"
            value={config.compatibleBaseUrl}
            placeholder="https://openrouter.ai/api/v1"
            spellCheck={false}
            onCommit={(compatibleBaseUrl) => save({ compatibleBaseUrl })}
          />
          <div className="flex flex-wrap gap-1">
            {COMPATIBLE_PRESETS.map((p) => (
              <Button
                key={`ai-preset-${p.label}`}
                variant="outline"
                size="xs"
                onClick={() => save({ compatibleBaseUrl: p.url })}
              >
                {p.label}
              </Button>
            ))}
          </div>
          <p className="text-[11px] text-muted-foreground">{t("ai.baseUrl.hint")}</p>
          {insecureRemote(config.compatibleBaseUrl) && (
            <p className="text-[11px] text-danger">{t("ai.baseUrl.insecure")}</p>
          )}
        </div>
      )}

      <div className="space-y-1.5">
        <div className="flex flex-wrap items-center gap-2">
          <label htmlFor="ai-key" className="text-xs font-medium">
            {t("ai.key.label")}
          </label>
          <span
            className={cn(
              "flex items-center gap-1 text-[11px]",
              hasKey ? "text-success" : "text-muted-foreground",
            )}
          >
            {hasKey ? <CircleCheck className="size-3" /> : <CircleX className="size-3" />}
            {hasKey ? t("ai.key.saved") : t("ai.key.none")}
          </span>
          {provider !== "compatible" && (
            <Button
              variant="link"
              size="xs"
              className="ml-auto h-auto px-0"
              onClick={() => void openExternal(KEY_PAGES[provider])}
            >
              <ExternalLink />
              {t("ai.key.getKey")}
            </Button>
          )}
        </div>
        <div className="flex gap-1.5">
          <div className="relative flex-1">
            <input
              id="ai-key"
              type={showKey ? "text" : "password"}
              value={key}
              autoComplete="off"
              spellCheck={false}
              placeholder={hasKey ? t("ai.key.replacePlaceholder") : t("ai.key.placeholder")}
              onChange={(e) => setKey(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && key.trim()) void saveKey();
              }}
              className="h-8 w-full rounded-md border bg-background pr-8 pl-2 font-mono text-xs focus:border-primary focus:outline-none"
            />
            <button
              type="button"
              className="absolute top-1/2 right-2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
              onClick={() => setShowKey((v) => !v)}
              aria-label={showKey ? t("ai.key.hide") : t("ai.key.show")}
              title={showKey ? t("ai.key.hide") : t("ai.key.show")}
            >
              {showKey ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
            </button>
          </div>
          <Button size="sm" disabled={!key.trim() || keyBusy} onClick={() => void saveKey()}>
            {keyBusy && <LoaderCircle className="animate-spin" />}
            {t("ai.key.save")}
          </Button>
          {hasKey && (
            <Button variant="outline" size="sm" onClick={() => setConfirmDelete(true)}>
              <Trash2 />
              {t("ai.key.delete")}
            </Button>
          )}
        </div>
        <p className="text-[11px] text-muted-foreground">
          {provider === "compatible" ? t("ai.key.optionalHint") : t("ai.key.hint")}
        </p>
        {keyError && <p className="text-xs text-danger">{keyError}</p>}
      </div>

      <div className="space-y-1.5">
        <label htmlFor="ai-model" className="text-xs font-medium">
          {t("ai.model.label")}
        </label>
        <LazyInput
          id="ai-model"
          list={listId}
          value={model}
          placeholder={suggestions[0]?.id}
          spellCheck={false}
          onCommit={(v) => save({ models: { ...config.models, [provider]: v } })}
        />
        <datalist id={listId}>
          {modelChoices.map((m) => (
            <option key={`${listId}-${m}`} value={m} />
          ))}
        </datalist>
        <div className="flex flex-wrap gap-1">
          {suggestions.map((s) => (
            <Button
              key={`ai-suggest-${s.id}`}
              variant={s.id === model ? "secondary" : "outline"}
              size="xs"
              title={t(`ai.modelNote.${s.note}`)}
              onClick={() => save({ models: { ...config.models, [provider]: s.id } })}
            >
              {s.id}
              <span className="text-muted-foreground">· {t(`ai.modelNote.${s.note}`)}</span>
            </Button>
          ))}
        </div>
      </div>

      <div className="space-y-1.5">
        <Button variant="outline" size="sm" onClick={() => void runTest()}>
          {testing ? <LoaderCircle className="animate-spin" /> : <PlugZap />}
          {testing ? t("ai.test.cancel") : t("ai.test.run")}
        </Button>
        {test && (
          <p
            className={cn(
              "flex items-start gap-1.5 text-xs",
              test.modelFound === false ? "text-warning" : "text-success",
            )}
          >
            {test.modelFound === false ? (
              <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
            ) : (
              <CircleCheck className="mt-0.5 size-3.5 shrink-0" />
            )}
            {test.modelFound === false
              ? t("ai.test.modelMissing", { model, n: test.models.length })
              : test.modelFound
                ? t("ai.test.okModel", { model })
                : t("ai.test.ok", { n: test.models.length })}
          </p>
        )}
        {testError && (
          <p className="flex items-start gap-1.5 text-xs text-danger">
            <CircleX className="mt-0.5 size-3.5 shrink-0" />
            {testError}
          </p>
        )}
      </div>

      <ConfirmDialog
        open={confirmDelete}
        title={t("ai.key.deleteTitle")}
        description={t("ai.key.deleteBody", { service: t(`ai.provider.${provider}.name`) })}
        confirmLabel={t("ai.key.delete")}
        destructive
        onConfirm={() => void deleteKey()}
        onCancel={() => setConfirmDelete(false)}
      />
    </section>
  );
}

function OptionsSection({
  config,
  save,
}: {
  config: FullAiConfig;
  save: (patch: Partial<FullAiConfig>) => void;
}) {
  const { t } = useTranslation();
  const languages: AiMessageLanguage[] = ["app", "en", "vi"];
  const styles: AiMessageStyle[] = ["conventional", "plain"];
  return (
    <section className="space-y-3">
      <div className="text-xs font-semibold">{t("ai.options.title")}</div>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1.5">
          <div className="text-xs font-medium">{t("ai.options.language")}</div>
          <Select
            value={config.language}
            onValueChange={(v) => save({ language: v as AiMessageLanguage })}
          >
            <SelectTrigger className="w-full text-xs" aria-label={t("ai.options.language")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {languages.map((l) => (
                <SelectItem key={`ai-lang-${l}`} value={l} className="text-xs">
                  {t(`ai.options.lang.${l}`)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1.5">
          <div className="text-xs font-medium">{t("ai.options.style")}</div>
          <Select value={config.style} onValueChange={(v) => save({ style: v as AiMessageStyle })}>
            <SelectTrigger className="w-full text-xs" aria-label={t("ai.options.style")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {styles.map((s) => (
                <SelectItem key={`ai-style-${s}`} value={s} className="text-xs">
                  {t(`ai.options.styles.${s}`)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>
      <div className="flex items-start gap-3 rounded-md border p-3 text-xs">
        <label htmlFor="ai-body" className="grid flex-1 cursor-pointer gap-0.5">
          <span className="font-medium">{t("ai.options.body")}</span>
          <span className="text-[11px] text-muted-foreground">{t("ai.options.bodyHint")}</span>
        </label>
        <Switch
          id="ai-body"
          checked={config.includeBody}
          onCheckedChange={(includeBody) => save({ includeBody })}
          className="mt-0.5"
        />
      </div>
      {isCloudProvider(config.provider) ? null : (
        <p className="text-[11px] text-muted-foreground">{t("ai.options.localHint")}</p>
      )}
    </section>
  );
}
