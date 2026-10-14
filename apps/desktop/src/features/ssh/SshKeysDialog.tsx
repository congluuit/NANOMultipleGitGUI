import { useState, type ReactNode } from "react";
import {
  Copy,
  FileKey,
  KeyRound,
  LoaderCircle,
  PlugZap,
  ShieldAlert,
  ShieldCheck,
  Star,
  Trash2,
  Upload,
  Wrench,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { homeDir, join } from "@tauri-apps/api/path";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { useAccounts, useProviders } from "@/features/accounts/queries";
import { commands, errorMessage, type Provider, type SshKeyInfo, type SshTestResult } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { cn } from "@/lib/utils";
import {
  accountName,
  importMessage,
  shortFingerprint,
  testMessage,
  uploadMessage,
  type Message,
  type Tone,
} from "./messages";
import { invalidateSshKeys, sshKeys, useSshKeyPermissions, useSshKeys } from "./queries";

const inputClass =
  "h-8 w-full rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none aria-invalid:border-destructive disabled:opacity-60";

const TONE_BOX: Record<Tone, string> = {
  ok: "border-success/40 bg-success/10",
  warn: "border-warning/50 bg-warning/10",
  danger: "border-destructive/50 bg-destructive/10 text-destructive",
};

const TONE_BADGE: Record<Tone | "muted", string> = {
  ok: "border-success/40 text-success",
  warn: "border-warning/50 text-warning",
  danger: "border-destructive/50 text-destructive",
  muted: "border-border text-muted-foreground",
};

function Badge({ tone = "muted", children }: { tone?: Tone | "muted"; children: ReactNode }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full border px-1.5 py-px text-[10px] leading-4 font-medium whitespace-nowrap",
        TONE_BADGE[tone],
      )}
    >
      {children}
    </span>
  );
}

function Notice({
  message,
  onSignIn,
  children,
}: {
  message: Message;
  onSignIn?: () => void;
  children?: ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <div
      role={message.tone === "danger" ? "alert" : "status"}
      className={cn("rounded-md border px-3 py-2 text-xs", TONE_BOX[message.tone])}
    >
      <div className="flex items-start gap-2">
        <span className="min-w-0 flex-1 break-words">{message.text}</span>
        {message.signIn && onSignIn && (
          <Button size="xs" variant="outline" onClick={onSignIn}>
            {t("ssh.uploadResult.signIn")}
          </Button>
        )}
      </div>
      {children}
    </div>
  );
}

/** SSH keys of one account — opened with `useAccountDialogs().openSshKeys(accountId)`. */
export function SshKeysDialog() {
  const target = useAccountDialogs((s) => s.sshKeys);
  const close = useAccountDialogs((s) => s.close);
  return (
    <Dialog open={target !== null} onOpenChange={(o) => !o && close("sshKeys")}>
      {target && <SshKeysContent key={target.accountId} accountId={target.accountId} />}
    </Dialog>
  );
}

type Form = { kind: "none" } | { kind: "generate" } | { kind: "import"; path: string };

export function SshKeysContent({ accountId }: { accountId: string }) {
  const { t } = useTranslation();
  const accounts = useAccounts().data;
  const providers = useProviders().data;
  const account = accounts?.find((a) => a.id === accountId) ?? null;
  const provider = providers?.find((p) => p.id === account?.providerId) ?? null;
  const keys = useSshKeys(accountId);
  const [form, setForm] = useState<Form>({ kind: "none" });
  const [notice, setNotice] = useState<Message | null>(null);

  const signIn = () => useAccountDialogs.getState().openManager({ accountId });

  const pickFile = async () => {
    setNotice(null);
    let defaultPath: string | undefined;
    try {
      defaultPath = await join(await homeDir(), ".ssh");
    } catch {
      defaultPath = undefined;
    }
    const path = await openFileDialog({
      multiple: false,
      directory: false,
      title: t("ssh.importForm.pick"),
      defaultPath,
    });
    if (typeof path === "string") setForm({ kind: "import", path });
  };

  const name = account ? accountName(account) : accountId;
  return (
    <DialogContent className="flex max-h-[85vh] flex-col sm:max-w-2xl">
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2">
          <KeyRound className="size-4" />
          {t("ssh.title", { account: name })}
        </DialogTitle>
        <DialogDescription>{t("ssh.description")}</DialogDescription>
      </DialogHeader>

      <div className="flex flex-wrap gap-2">
        <Button
          size="sm"
          variant={form.kind === "generate" ? "secondary" : "default"}
          onClick={() => {
            setNotice(null);
            setForm(form.kind === "generate" ? { kind: "none" } : { kind: "generate" });
          }}
        >
          <KeyRound />
          {t("ssh.generate")}
        </Button>
        <Button size="sm" variant="outline" onClick={() => void pickFile()}>
          <FileKey />
          {t("ssh.import")}
        </Button>
      </div>

      {form.kind === "generate" && (
        <GenerateForm
          accountId={accountId}
          onCancel={() => setForm({ kind: "none" })}
          onDone={() => {
            setForm({ kind: "none" });
            setNotice({ tone: "ok", text: t("ssh.generateForm.done") });
          }}
        />
      )}
      {form.kind === "import" && (
        <ImportForm
          accountId={accountId}
          path={form.path}
          onCancel={() => setForm({ kind: "none" })}
          onDone={(message) => {
            setForm({ kind: "none" });
            setNotice(message ?? { tone: "ok", text: t("ssh.importForm.done") });
          }}
        />
      )}
      {notice && <Notice message={notice} onSignIn={signIn} />}

      <div className="-mx-1 min-h-0 flex-1 overflow-y-auto px-1">
        {keys.isLoading ? (
          <p className="py-6 text-center text-xs text-muted-foreground">{t("ssh.loading")}</p>
        ) : keys.error ? (
          <Notice message={{ tone: "danger", text: errorMessage(keys.error) }} />
        ) : !keys.data?.length ? (
          <p className="rounded-md border border-dashed py-6 text-center text-xs text-muted-foreground">
            {t("ssh.empty")}
          </p>
        ) : (
          <ul className="space-y-2" aria-label={t("ssh.title", { account: name })}>
            {keys.data.map((k) => (
              <KeyRow
                key={k.id}
                sshKey={k}
                accountId={accountId}
                provider={provider}
                onSignIn={signIn}
              />
            ))}
          </ul>
        )}
      </div>
    </DialogContent>
  );
}

function GenerateForm({
  accountId,
  onCancel,
  onDone,
}: {
  accountId: string;
  onCancel: () => void;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const [comment, setComment] = useState("");
  const [pass, setPass] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mismatch = pass !== confirm;

  const submit = async () => {
    if (mismatch || busy) return;
    setBusy(true);
    setError(null);
    try {
      await commands.sshKeyGenerate(accountId, comment.trim() || null, pass || null);
      invalidateSshKeys();
      onDone();
    } catch (e) {
      setError(t("ssh.error", { message: errorMessage(e) }));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="grid gap-2 rounded-md border bg-muted/30 p-3"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <p className="text-xs font-semibold">{t("ssh.generateForm.title")}</p>
      <label className="grid gap-1 text-xs">
        <span className="text-muted-foreground">{t("ssh.generateForm.comment")}</span>
        <input
          className={inputClass}
          value={comment}
          onChange={(e) => setComment(e.target.value)}
          placeholder={t("ssh.generateForm.commentHint")}
        />
      </label>
      <div className="grid gap-2 sm:grid-cols-2">
        <label className="grid gap-1 text-xs">
          <span className="text-muted-foreground">{t("ssh.generateForm.passphrase")}</span>
          <input
            type="password"
            autoComplete="new-password"
            className={inputClass}
            value={pass}
            onChange={(e) => setPass(e.target.value)}
          />
        </label>
        <label className="grid gap-1 text-xs">
          <span className="text-muted-foreground">{t("ssh.generateForm.confirm")}</span>
          <input
            type="password"
            autoComplete="new-password"
            className={inputClass}
            value={confirm}
            aria-invalid={mismatch}
            onChange={(e) => setConfirm(e.target.value)}
          />
        </label>
      </div>
      <p className={cn("text-[11px]", mismatch ? "text-destructive" : "text-muted-foreground")}>
        {mismatch ? t("ssh.generateForm.mismatch") : t("ssh.generateForm.passphraseHint")}
      </p>
      {error && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}
      <div className="flex justify-end gap-2">
        <Button type="button" size="sm" variant="outline" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
        <Button type="submit" size="sm" disabled={mismatch || busy}>
          {busy && <LoaderCircle className="animate-spin" />}
          {t("ssh.generateForm.submit")}
        </Button>
      </div>
    </form>
  );
}

function ImportForm({
  accountId,
  path,
  onCancel,
  onDone,
}: {
  accountId: string;
  path: string;
  onCancel: () => void;
  onDone: (message: Message | null) => void;
}) {
  const { t } = useTranslation();
  const [pass, setPass] = useState("");
  const [needPass, setNeedPass] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<Message | null>(null);

  const submit = async () => {
    if (busy) return;
    setBusy(true);
    setMessage(null);
    try {
      const r = await commands.sshKeyImport(accountId, path, pass || null);
      if (r.kind === "imported") {
        invalidateSshKeys();
        onDone(importMessage(t, r, accountId));
        return;
      }
      if (r.kind === "passphraseRequired" || r.kind === "wrongPassphrase") setNeedPass(true);
      setMessage(importMessage(t, r, accountId));
    } catch (e) {
      setMessage({ tone: "danger", text: t("ssh.error", { message: errorMessage(e) }) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="grid gap-2 rounded-md border bg-muted/30 p-3"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <p className="text-xs font-semibold">{t("ssh.importForm.title")}</p>
      <div className="grid gap-1 text-xs">
        <span className="text-muted-foreground">{t("ssh.importForm.file")}</span>
        <code className="rounded bg-muted px-2 py-1 font-mono text-[11px] break-all">{path}</code>
      </div>
      {needPass && (
        <label className="grid gap-1 text-xs">
          <span className="text-muted-foreground">{t("ssh.importForm.passphrase")}</span>
          <input
            type="password"
            autoFocus
            autoComplete="current-password"
            className={inputClass}
            value={pass}
            onChange={(e) => setPass(e.target.value)}
          />
        </label>
      )}
      <p className="text-[11px] text-muted-foreground">{t("ssh.importForm.note")}</p>
      {message && <Notice message={message} />}
      <div className="flex justify-end gap-2">
        <Button type="button" size="sm" variant="outline" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
        <Button type="submit" size="sm" disabled={busy || (needPass && !pass)}>
          {busy && <LoaderCircle className="animate-spin" />}
          {t("ssh.importForm.submit")}
        </Button>
      </div>
    </form>
  );
}

function KeyRow({
  sshKey: k,
  accountId,
  provider,
  onSignIn,
}: {
  sshKey: SshKeyInfo;
  accountId: string;
  provider: Provider | null;
  onSignIn: () => void;
}) {
  const { t, i18n } = useTranslation();
  const perms = useSshKeyPermissions(k.id, k.fileExists);
  const [busy, setBusy] = useState<null | "upload" | "default" | "test" | "repair" | "delete">(
    null,
  );
  const [message, setMessage] = useState<Message | null>(null);
  const [test, setTest] = useState<SshTestResult | null>(null);
  const [uploadTitle, setUploadTitle] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [copied, setCopied] = useState(false);

  const run = async (kind: NonNullable<typeof busy>, f: () => Promise<void>) => {
    setBusy(kind);
    setMessage(null);
    setTest(null);
    try {
      await f();
    } catch (e) {
      setMessage({ tone: "danger", text: t("ssh.error", { message: errorMessage(e) }) });
    } finally {
      setBusy(null);
    }
  };

  const copy = () => {
    void navigator.clipboard.writeText(k.publicKey).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  const upload = (title: string) =>
    run("upload", async () => {
      const r = await commands.sshKeyUpload(k.id, title);
      setMessage(uploadMessage(t, r));
      setUploadTitle(null);
      invalidateSshKeys();
    });

  const setDefault = () =>
    run("default", async () => {
      await commands.sshKeySetDefault(accountId, k.id);
      invalidateSshKeys();
    });

  const runTest = () =>
    run("test", async () => {
      const r = await commands.sshTestConnection(accountId, k.id);
      setTest(r);
      setMessage(testMessage(t, r));
    });

  const repair = () =>
    run("repair", async () => {
      const r = await commands.sshKeyRepairPermissions(k.id);
      queryClient.setQueryData(sshKeys.permissions(k.id), r);
    });

  const remove = () =>
    run("delete", async () => {
      await commands.sshKeyDelete(k.id);
      setConfirmDelete(false);
      invalidateSshKeys();
    });

  const permsBad = perms.data && !perms.data.restricted;
  const created = k.createdAt
    ? new Date(k.createdAt).toLocaleDateString(i18n.language === "vi" ? "vi-VN" : "en-US")
    : null;

  return (
    <li
      className={cn("rounded-md border p-3 text-xs", k.isDefault && "border-primary/60")}
      data-testid={`ssh-key-${k.id}`}
    >
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="font-mono text-[11px] font-semibold uppercase">{k.keyType}</span>
        <span className="min-w-0 truncate font-medium" title={k.comment ?? undefined}>
          {k.comment || "—"}
        </span>
        {k.isDefault && (
          <Badge tone="ok">
            <Star className="size-2.5" />
            {t("ssh.badge.default")}
          </Badge>
        )}
        {k.uploadedToProvider ? (
          <Badge tone="ok">{t("ssh.badge.uploaded")}</Badge>
        ) : (
          <Badge>{t("ssh.badge.notUploaded")}</Badge>
        )}
        {k.hasPassphrase && (
          <Badge tone={k.passphraseSaved ? "muted" : "warn"}>
            {k.passphraseSaved ? t("ssh.badge.passphrase") : t("ssh.badge.passphraseUnsaved")}
          </Badge>
        )}
        {!k.fileExists && <Badge tone="danger">{t("ssh.badge.fileMissing")}</Badge>}
        {!k.managed && k.fileExists && <Badge>{t("ssh.badge.external")}</Badge>}
        {perms.data &&
          (perms.data.restricted ? (
            <Badge tone="ok">
              <ShieldCheck className="size-2.5" />
              {t("ssh.badge.permsOk")}
            </Badge>
          ) : (
            <Badge tone="danger">
              <ShieldAlert className="size-2.5" />
              {t("ssh.badge.permsBad")}
            </Badge>
          ))}
      </div>
      <div
        className="mt-1 font-mono text-[11px] break-all text-muted-foreground"
        title={k.fingerprint}
      >
        {shortFingerprint(k.fingerprint, 16)}
        {created && <span className="ml-2 font-sans">· {t("ssh.created", { date: created })}</span>}
      </div>
      {permsBad && perms.data?.problem && (
        <p className="mt-1 text-[11px] text-destructive">{perms.data.problem}</p>
      )}

      <div className="mt-2 flex flex-wrap gap-1.5">
        <Button size="xs" variant="outline" onClick={copy}>
          <Copy />
          {copied ? t("ssh.copied") : t("ssh.copy")}
        </Button>
        <Button
          size="xs"
          variant="outline"
          disabled={busy !== null}
          onClick={() =>
            setUploadTitle(uploadTitle === null ? t("ssh.uploadForm.defaultTitle") : null)
          }
        >
          <Upload />
          {t("ssh.upload")}
        </Button>
        {!k.isDefault && (
          <Button
            size="xs"
            variant="outline"
            disabled={busy !== null || !k.fileExists}
            onClick={() => void setDefault()}
          >
            <Star />
            {t("ssh.setDefault")}
          </Button>
        )}
        <Button
          size="xs"
          variant="outline"
          disabled={busy !== null || !k.fileExists}
          onClick={() => void runTest()}
        >
          {busy === "test" ? <LoaderCircle className="animate-spin" /> : <PlugZap />}
          {busy === "test" ? t("ssh.testing") : t("ssh.test")}
        </Button>
        {permsBad && perms.data?.repairable && (
          <Button
            size="xs"
            variant="outline"
            disabled={busy !== null}
            onClick={() => void repair()}
          >
            <Wrench />
            {t("ssh.repair")}
          </Button>
        )}
        <Button
          size="xs"
          variant="ghost"
          className="ml-auto text-destructive hover:text-destructive"
          disabled={busy !== null}
          onClick={() => setConfirmDelete(true)}
        >
          <Trash2 />
          {t("ssh.delete")}
        </Button>
      </div>

      {uploadTitle !== null && (
        <form
          className="mt-2 grid gap-1.5 rounded-md border bg-muted/30 p-2"
          onSubmit={(e) => {
            e.preventDefault();
            void upload(uploadTitle);
          }}
        >
          <label className="grid gap-1">
            <span className="text-muted-foreground">
              {t("ssh.uploadForm.title", { provider: provider?.name ?? "provider" })}
            </span>
            <input
              className={inputClass}
              value={uploadTitle}
              aria-label={t("ssh.uploadForm.keyTitle")}
              onChange={(e) => setUploadTitle(e.target.value)}
            />
          </label>
          <div className="flex justify-end gap-2">
            <Button type="button" size="xs" variant="outline" onClick={() => setUploadTitle(null)}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="xs" disabled={busy !== null}>
              {busy === "upload" && <LoaderCircle className="animate-spin" />}
              {t("ssh.uploadForm.submit")}
            </Button>
          </div>
        </form>
      )}

      {message && (
        <div className="mt-2">
          <Notice message={message} onSignIn={onSignIn}>
            {test?.output && (
              <details className="mt-1">
                <summary className="cursor-pointer text-[11px] text-muted-foreground">
                  {t("ssh.testResult.details")}
                </summary>
                <pre className="mt-1 max-h-32 overflow-auto font-mono text-[10px] whitespace-pre-wrap">
                  {test.output}
                </pre>
              </details>
            )}
          </Notice>
        </div>
      )}

      <ConfirmDialog
        open={confirmDelete}
        title={t("ssh.deleteConfirm.title")}
        description={t("ssh.deleteConfirm.body", { fingerprint: shortFingerprint(k.fingerprint) })}
        confirmLabel={t("ssh.deleteConfirm.confirm")}
        destructive
        busy={busy === "delete"}
        onConfirm={() => void remove()}
        onCancel={() => setConfirmDelete(false)}
      />
    </li>
  );
}
