import { useState } from "react";
import { LoaderCircle, Pencil, Plus, Star, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  commands,
  errorMessage,
  type Identity,
  type ProviderEmail,
  type SigningMethod,
} from "@/ipc";
import { useIdentities } from "../queries";
import {
  emptyIdentity,
  identityToForm,
  validateIdentity,
  type IdentityErrors,
  type IdentityForm,
} from "./forms";
import { refreshAccounts, useAccountEmails } from "./queries";
import { Badge, Callout, Field, TextInput, inputClass } from "./ui";

/** Identities (commit name/email/signing) of one account. */
export function IdentitiesSection({
  accountId,
  hasToken,
}: {
  accountId: string;
  hasToken: boolean;
}) {
  const { t } = useTranslation();
  const identities = useIdentities(accountId);
  const emails = useAccountEmails(hasToken ? accountId : null);
  const [editing, setEditing] = useState<Identity | "new" | null>(null);
  const [deleting, setDeleting] = useState<Identity | null>(null);
  const [error, setError] = useState<string | null>(null);
  const list = identities.data ?? [];

  const run = async (action: () => Promise<unknown>) => {
    setError(null);
    try {
      await action();
      refreshAccounts();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <div className="grid gap-2">
      <div className="flex items-center justify-between">
        <h4 className="text-xs font-semibold">{t("accounts.identities.title")}</h4>
        {editing === null && (
          <Button size="xs" variant="outline" onClick={() => setEditing("new")}>
            <Plus />
            {t("accounts.identities.add")}
          </Button>
        )}
      </div>
      {list.length === 0 && editing === null && (
        <p className="text-[11px] text-muted-foreground">{t("accounts.identities.empty")}</p>
      )}
      <ul className="grid gap-1">
        {list.map((i) =>
          editing !== "new" && editing?.id === i.id ? (
            <li key={i.id}>
              <IdentityFormView
                initial={identityToForm(i)}
                others={list.filter((o) => o.id !== i.id)}
                emails={emails.data ?? []}
                onCancel={() => setEditing(null)}
                onSubmit={async (form) => {
                  await commands.identitySave({ id: i.id, accountId, ...toInput(form) });
                  refreshAccounts();
                  setEditing(null);
                }}
              />
            </li>
          ) : (
            <li
              key={i.id}
              className="flex items-center gap-2 rounded-md border px-2 py-1.5 text-xs"
            >
              <span className="min-w-0 flex-1 truncate">
                {i.name} <span className="text-muted-foreground">&lt;{i.email}&gt;</span>
              </span>
              {i.isDefault && <Badge tone="ok">{t("accounts.identities.default")}</Badge>}
              {i.signing !== "none" && (
                <Badge>{t(`accounts.identities.signing.${i.signing}`)}</Badge>
              )}
              {!i.isDefault && (
                <Button
                  size="icon-xs"
                  variant="ghost"
                  aria-label={t("accounts.identities.setDefault")}
                  title={t("accounts.identities.setDefault")}
                  onClick={() => void run(() => commands.identitySetDefault(i.id))}
                >
                  <Star />
                </Button>
              )}
              <Button
                size="icon-xs"
                variant="ghost"
                aria-label={t("accounts.identities.edit")}
                title={t("accounts.identities.edit")}
                onClick={() => setEditing(i)}
              >
                <Pencil />
              </Button>
              <Button
                size="icon-xs"
                variant="ghost"
                aria-label={t("accounts.identities.delete")}
                title={t("accounts.identities.delete")}
                onClick={() => setDeleting(i)}
              >
                <Trash2 />
              </Button>
            </li>
          ),
        )}
      </ul>
      {editing === "new" && (
        <IdentityFormView
          initial={emptyIdentity(list.length === 0)}
          others={list}
          emails={emails.data ?? []}
          onCancel={() => setEditing(null)}
          onSubmit={async (form) => {
            await commands.identitySave({ accountId, ...toInput(form) });
            refreshAccounts();
            setEditing(null);
          }}
        />
      )}
      {error && <Callout tone="danger">{error}</Callout>}
      <ConfirmDialog
        open={deleting !== null}
        title={t("accounts.identities.deleteTitle")}
        description={t("accounts.identities.deleteBody", {
          identity: deleting ? `${deleting.name} <${deleting.email}>` : "",
        })}
        confirmLabel={t("accounts.identities.delete")}
        destructive
        onCancel={() => setDeleting(null)}
        onConfirm={() => {
          const target = deleting;
          setDeleting(null);
          if (target) void run(() => commands.identityDelete(target.id));
        }}
      />
    </div>
  );
}

function toInput(form: IdentityForm) {
  return {
    name: form.name.trim(),
    email: form.email.trim(),
    signing: form.signing,
    signingKeyRef: form.signing === "none" ? null : form.signingKeyRef.trim(),
    isDefault: form.isDefault,
  };
}

const SIGNING: SigningMethod[] = ["none", "gpg", "ssh"];

/** Add/edit form of an identity, with verified provider emails as suggestions. */
export function IdentityFormView({
  initial,
  others,
  emails,
  onSubmit,
  onCancel,
}: {
  initial: IdentityForm;
  others: readonly Identity[];
  emails: readonly ProviderEmail[];
  onSubmit: (form: IdentityForm) => Promise<void>;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [form, setForm] = useState(initial);
  const [errors, setErrors] = useState<IdentityErrors>({});
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const set = (patch: Partial<IdentityForm>) => setForm((f) => ({ ...f, ...patch }));
  const suggestions = emails.filter(
    (e) => e.verified && !others.some((o) => o.email.toLowerCase() === e.email.toLowerCase()),
  );

  const submit = async () => {
    const found = validateIdentity(form, others);
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

  return (
    <form
      aria-label={t("accounts.identities.form")}
      className="grid gap-2 rounded-md border p-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (!busy) void submit();
      }}
    >
      <div className="grid grid-cols-2 gap-2">
        <Field
          label={t("accounts.identities.name")}
          error={errors.name && t(`accounts.identities.error.name.${errors.name}`)}
        >
          <TextInput
            value={form.name}
            onChange={(e) => set({ name: e.target.value })}
            aria-invalid={Boolean(errors.name)}
            autoFocus
          />
        </Field>
        <Field
          label={t("accounts.identities.email")}
          error={errors.email && t(`accounts.identities.error.email.${errors.email}`)}
        >
          <TextInput
            type="email"
            value={form.email}
            list={suggestions.length > 0 ? "identity-email-suggestions" : undefined}
            onChange={(e) => set({ email: e.target.value })}
            aria-invalid={Boolean(errors.email)}
          />
        </Field>
      </div>
      {suggestions.length > 0 && (
        <>
          <datalist id="identity-email-suggestions">
            {suggestions.map((e) => (
              <option key={e.email} value={e.email} />
            ))}
          </datalist>
          <div className="flex flex-wrap items-center gap-1 text-[11px] text-muted-foreground">
            {t("accounts.identities.verified")}:
            {suggestions.map((e) => (
              <button
                key={e.email}
                type="button"
                className="rounded border px-1.5 hover:bg-accent"
                onClick={() => set({ email: e.email })}
              >
                {e.email}
              </button>
            ))}
          </div>
        </>
      )}
      <div className="grid grid-cols-[8rem_1fr] gap-2">
        <Field label={t("accounts.identities.signingLabel")}>
          <select
            className={inputClass}
            value={form.signing}
            onChange={(e) => set({ signing: e.target.value as SigningMethod })}
          >
            {SIGNING.map((s) => (
              <option key={s} value={s}>
                {t(`accounts.identities.signing.${s}`)}
              </option>
            ))}
          </select>
        </Field>
        {form.signing !== "none" && (
          <Field
            label={t("accounts.identities.keyRef")}
            hint={t(`accounts.identities.keyRefHint.${form.signing}`)}
            error={errors.signingKeyRef && t("accounts.identities.error.keyRef")}
          >
            <TextInput
              value={form.signingKeyRef}
              onChange={(e) => set({ signingKeyRef: e.target.value })}
              aria-invalid={Boolean(errors.signingKeyRef)}
            />
          </Field>
        )}
      </div>
      <label className="flex items-center gap-2 text-xs">
        <input
          type="checkbox"
          className="accent-primary"
          checked={form.isDefault}
          onChange={(e) => set({ isDefault: e.target.checked })}
        />
        {t("accounts.identities.makeDefault")}
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
