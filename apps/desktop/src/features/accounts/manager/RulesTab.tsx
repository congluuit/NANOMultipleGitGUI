import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  ChevronDown,
  ChevronUp,
  FlaskConical,
  FolderOpen,
  LoaderCircle,
  Pencil,
  Plus,
  Trash2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  commands,
  errorMessage,
  type Account,
  type Identity,
  type Rule,
  type RuleTestResult,
} from "@/ipc";
import { cn } from "@/lib/utils";
import {
  emptyRule,
  moveRule,
  ruleInput,
  ruleToForm,
  sortRules,
  validateRule,
  type RuleErrors,
  type RuleForm,
} from "./forms";
import { refreshAccounts } from "./queries";
import { Badge, Callout, Field, TextInput, inputClass } from "./ui";

const accountName = (a: Account | undefined) => (a ? (a.label ?? a.username) : "?");

/** Rule engine settings: ordered list (first match wins), add/edit, and a "Test" box. */
export function RulesTab({
  rules,
  accounts,
  identities,
}: {
  rules: readonly Rule[];
  accounts: readonly Account[];
  identities: readonly Identity[];
}) {
  const { t } = useTranslation();
  const ordered = sortRules(rules);
  const [editing, setEditing] = useState<Rule | "new" | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = async (action: () => Promise<unknown>) => {
    setError(null);
    try {
      await action();
      refreshAccounts();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const move = (index: number, delta: number) => {
    const ids = ordered.map((r) => r.id);
    const next = moveRule(ids, index, delta);
    if (next.join() !== ids.join()) void run(() => commands.ruleReorder(next));
  };

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-[11px] text-muted-foreground">{t("accounts.rules.hint")}</p>
        {editing === null && (
          <Button size="sm" disabled={accounts.length === 0} onClick={() => setEditing("new")}>
            <Plus />
            {t("accounts.rules.add")}
          </Button>
        )}
      </div>
      {editing !== null && (
        <RuleFormView
          initial={editing === "new" ? emptyRule(accounts[0]?.id ?? "") : ruleToForm(editing)}
          accounts={accounts}
          identities={identities}
          onCancel={() => setEditing(null)}
          onSubmit={async (form) => {
            await commands.ruleSave(ruleInput(form, editing === "new" ? null : editing.id));
            refreshAccounts();
            setEditing(null);
          }}
        />
      )}
      {ordered.length === 0 && editing === null && <Callout>{t("accounts.rules.empty")}</Callout>}
      {error && <Callout tone="danger">{error}</Callout>}
      <ol aria-label={t("accounts.rules.title")} className="grid gap-1">
        {ordered.map((r, index) => (
          <li
            key={r.id}
            className={cn(
              "flex items-center gap-2 rounded-md border px-2 py-1.5 text-xs",
              !r.enabled && "opacity-60",
            )}
          >
            <span className="w-5 text-right font-mono text-[10px] text-muted-foreground">
              {index + 1}
            </span>
            <span className="flex min-w-0 flex-1 flex-wrap items-center gap-1">
              <RuleConditions rule={r} />
              <span className="text-muted-foreground">→</span>
              <span className="font-medium">
                {accountName(accounts.find((a) => a.id === r.accountId))}
              </span>
              {r.identityId && (
                <span className="truncate text-muted-foreground">
                  ({identities.find((i) => i.id === r.identityId)?.email})
                </span>
              )}
              {!r.enabled && <Badge>{t("accounts.rules.disabled")}</Badge>}
            </span>
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label={t("accounts.rules.moveUp")}
              disabled={index === 0}
              onClick={() => move(index, -1)}
            >
              <ChevronUp />
            </Button>
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label={t("accounts.rules.moveDown")}
              disabled={index === ordered.length - 1}
              onClick={() => move(index, 1)}
            >
              <ChevronDown />
            </Button>
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label={t("accounts.rules.edit")}
              onClick={() => setEditing(r)}
            >
              <Pencil />
            </Button>
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label={t("accounts.rules.delete")}
              onClick={() => void run(() => commands.ruleDelete(r.id))}
            >
              <Trash2 />
            </Button>
          </li>
        ))}
      </ol>
      <RuleTester accounts={accounts} />
    </div>
  );
}

function RuleConditions({ rule }: { rule: Rule }) {
  const { t } = useTranslation();
  const parts: [string, string][] = [];
  if (rule.host) parts.push([t("accounts.rules.host"), rule.host]);
  if (rule.ownerOrGroup) parts.push([t("accounts.rules.owner"), rule.ownerOrGroup]);
  if (rule.pathGlob) parts.push([t("accounts.rules.path"), rule.pathGlob]);
  if (rule.remoteRegex) parts.push([t("accounts.rules.regex"), rule.remoteRegex]);
  return (
    <>
      {parts.map(([k, v]) => (
        <span key={k} className="rounded bg-muted px-1 font-mono text-[11px]" title={k}>
          {v}
        </span>
      ))}
    </>
  );
}

export function RuleFormView({
  initial,
  accounts,
  identities,
  onSubmit,
  onCancel,
}: {
  initial: RuleForm;
  accounts: readonly Account[];
  identities: readonly Identity[];
  onSubmit: (form: RuleForm) => Promise<void>;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [form, setForm] = useState(initial);
  const [errors, setErrors] = useState<RuleErrors>({});
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const set = (patch: Partial<RuleForm>) => setForm((f) => ({ ...f, ...patch }));
  const accountIdentities = identities.filter((i) => i.accountId === form.accountId);

  const submit = async () => {
    const found = validateRule(form);
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    setBusy(true);
    setFailure(null);
    try {
      await onSubmit(form);
    } catch (e) {
      setFailure(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const pick = async () => {
    const dir = await openDialog({ directory: true, multiple: false });
    if (typeof dir === "string") set({ pathGlob: `${dir.replace(/\\/g, "/")}/**` });
  };

  return (
    <form
      aria-label={t("accounts.rules.form")}
      className="grid gap-2 rounded-md border p-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (!busy) void submit();
      }}
    >
      <p className="text-[11px] text-muted-foreground">{t("accounts.rules.formHint")}</p>
      <div className="grid grid-cols-2 gap-2">
        <Field label={t("accounts.rules.host")}>
          <TextInput
            value={form.host}
            placeholder="github.com"
            onChange={(e) => set({ host: e.target.value })}
          />
        </Field>
        <Field label={t("accounts.rules.owner")}>
          <TextInput
            value={form.ownerOrGroup}
            placeholder="acme-corp"
            onChange={(e) => set({ ownerOrGroup: e.target.value })}
          />
        </Field>
        <Field label={t("accounts.rules.path")}>
          <div className="flex gap-1">
            <TextInput
              value={form.pathGlob}
              placeholder="D:/Work/**"
              onChange={(e) => set({ pathGlob: e.target.value })}
            />
            <Button
              type="button"
              size="icon-sm"
              variant="outline"
              aria-label={t("accounts.field.browse")}
              onClick={() => void pick()}
            >
              <FolderOpen />
            </Button>
          </div>
        </Field>
        <Field
          label={t("accounts.rules.regex")}
          error={errors.remoteRegex && t("accounts.rules.error.regex")}
        >
          <TextInput
            value={form.remoteRegex}
            placeholder="gitlab\.acme\.vn/.*"
            spellCheck={false}
            onChange={(e) => set({ remoteRegex: e.target.value })}
            aria-invalid={Boolean(errors.remoteRegex)}
          />
        </Field>
      </div>
      {errors.match && (
        <p role="alert" className="text-[11px] text-destructive">
          {t("accounts.rules.error.match")}
        </p>
      )}
      <div className="grid grid-cols-2 gap-2">
        <Field
          label={t("accounts.rules.account")}
          error={errors.accountId && t("accounts.rules.error.account")}
        >
          <select
            className={inputClass}
            value={form.accountId}
            onChange={(e) => set({ accountId: e.target.value, identityId: "" })}
          >
            {accounts.map((a) => (
              <option key={a.id} value={a.id}>
                {accountName(a)} (@{a.username})
              </option>
            ))}
          </select>
        </Field>
        <Field label={t("accounts.rules.identity")}>
          <select
            className={inputClass}
            value={form.identityId}
            onChange={(e) => set({ identityId: e.target.value })}
          >
            <option value="">{t("accounts.rules.defaultIdentity")}</option>
            {accountIdentities.map((i) => (
              <option key={i.id} value={i.id}>
                {i.name} &lt;{i.email}&gt;
              </option>
            ))}
          </select>
        </Field>
      </div>
      <label className="flex items-center gap-2 text-xs">
        <input
          type="checkbox"
          className="accent-primary"
          checked={form.enabled}
          onChange={(e) => set({ enabled: e.target.checked })}
        />
        {t("accounts.rules.enabled")}
      </label>
      {failure && <Callout tone="danger">{failure}</Callout>}
      <div className="flex justify-end gap-2">
        <Button type="button" size="xs" variant="outline" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
        <Button type="submit" size="xs" disabled={busy}>
          {busy && <LoaderCircle className="animate-spin" />}
          {t("accounts.save")}
        </Button>
      </div>
    </form>
  );
}

function RuleTester({ accounts }: { accounts: readonly Account[] }) {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");
  const [path, setPath] = useState("");
  const [result, setResult] = useState<RuleTestResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const test = async () => {
    setError(null);
    try {
      setResult(await commands.ruleTest(url.trim() || null, path.trim() || null));
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <form
      aria-label={t("accounts.rules.test")}
      className="grid gap-2 rounded-md border border-dashed p-3"
      onSubmit={(e) => {
        e.preventDefault();
        void test();
      }}
    >
      <h4 className="flex items-center gap-1.5 text-xs font-semibold">
        <FlaskConical className="size-3.5" />
        {t("accounts.rules.test")}
      </h4>
      <div className="grid grid-cols-[1fr_1fr_auto] items-end gap-2">
        <Field label={t("accounts.rules.testUrl")}>
          <TextInput
            value={url}
            placeholder="git@github.com:acme-corp/api.git"
            onChange={(e) => setUrl(e.target.value)}
          />
        </Field>
        <Field label={t("accounts.rules.testPath")}>
          <TextInput
            value={path}
            placeholder="D:/Work/api"
            onChange={(e) => setPath(e.target.value)}
          />
        </Field>
        <Button type="submit" size="sm" variant="outline" disabled={!url.trim() && !path.trim()}>
          {t("accounts.rules.testRun")}
        </Button>
      </div>
      {error && <Callout tone="danger">{error}</Callout>}
      {result && (
        <p role="status" className="text-xs">
          {result.matchedRule && result.account
            ? t("accounts.rules.testMatch", {
                account: accountName(
                  accounts.find((a) => a.id === result.account?.id) ?? result.account,
                ),
                identity: result.identity
                  ? result.identity.email
                  : t("accounts.rules.defaultIdentity"),
                matches: result.allMatches.length,
              })
            : t("accounts.rules.testNone")}
        </p>
      )}
    </form>
  );
}
