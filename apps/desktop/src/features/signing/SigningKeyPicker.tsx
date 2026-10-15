import { useState } from "react";
import { FileKey, FlaskConical, LoaderCircle } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { commands, errorMessage, type SignTestResult, type SigningMethod } from "@/ipc";
import { useSshKeys } from "@/features/ssh/queries";
import { cn } from "@/lib/utils";
import { gpgKeyLabel, sshKeyLabel, sshKeyRef, usableGpgKeys } from "./model";

const selectClass =
  "h-7 min-w-0 flex-1 rounded-md border bg-background px-1.5 text-xs focus:border-primary focus:outline-none";

/**
 * Helpers under the "Signing key" field of an identity: pick a GPG secret key (listed with the
 * gpg git uses) or one of the account's SSH keys / a `.pub` file, and "Thử ký" (sign a test
 * commit with exactly what git will run).
 */
export function SigningKeyPicker({
  accountId,
  signing,
  keyRef,
  email,
  onPick,
}: {
  accountId?: string;
  signing: SigningMethod;
  keyRef: string;
  email: string;
  onPick: (ref: string) => void;
}) {
  const { t } = useTranslation();
  const gpg = useQuery({
    queryKey: ["signing", "gpgKeys"],
    queryFn: () => commands.signingGpgKeys(),
    enabled: isTauri() && signing === "gpg",
    staleTime: 60_000,
  });
  const ssh = useSshKeys(accountId ?? null, signing === "ssh");
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<SignTestResult | { error: string } | null>(null);

  if (signing === "none") return null;

  const test = async () => {
    setTesting(true);
    setResult(null);
    try {
      setResult(
        await commands.signingTest({
          accountId: accountId ?? null,
          signing,
          signingKeyRef: keyRef.trim(),
          email: email.trim() || null,
        }),
      );
    } catch (e) {
      setResult({ error: errorMessage(e) });
    } finally {
      setTesting(false);
    }
  };

  const pickFile = async () => {
    if (!isTauri()) return;
    const file = await openDialog({
      multiple: false,
      title: t("signing.pickFile"),
      filters: [{ name: t("signing.pubFiles"), extensions: ["pub"] }],
    });
    if (typeof file === "string") onPick(file);
  };

  return (
    <div className="grid gap-1.5 text-xs">
      <div className="flex items-center gap-1.5">
        {signing === "gpg" &&
          (gpg.data?.status === "notInstalled" ? (
            <span className="flex-1 text-[11px] text-warning">
              {t("signing.gpgMissing", { program: gpg.data.program })}
            </span>
          ) : (
            <select
              aria-label={t("signing.pickGpg")}
              className={selectClass}
              value=""
              onChange={(e) => e.target.value && onPick(e.target.value)}
            >
              <option value="">
                {gpg.isFetching
                  ? t("signing.loading")
                  : gpg.data?.status === "found" && gpg.data.keys.length === 0
                    ? t("signing.noGpgKeys")
                    : t("signing.pickGpg")}
              </option>
              {gpg.data?.status === "found" &&
                usableGpgKeys(gpg.data.keys).map((k) => (
                  <option key={k.keyId} value={k.fingerprint ?? k.keyId} disabled={!k.canSign}>
                    {gpgKeyLabel(k)}
                    {k.canSign ? "" : ` (${t("signing.cannotSign")})`}
                  </option>
                ))}
            </select>
          ))}
        {signing === "ssh" && (
          <>
            <select
              aria-label={t("signing.pickSsh")}
              className={selectClass}
              value=""
              onChange={(e) => e.target.value && onPick(e.target.value)}
            >
              <option value="">
                {(ssh.data ?? []).length === 0 ? t("signing.noSshKeys") : t("signing.pickSsh")}
              </option>
              {(ssh.data ?? []).map((k) => {
                const ref = sshKeyRef(k);
                return ref ? (
                  <option key={k.id} value={ref}>
                    {sshKeyLabel(k)}
                  </option>
                ) : null;
              })}
            </select>
            <Button type="button" size="xs" variant="outline" onClick={() => void pickFile()}>
              <FileKey />
              {t("signing.pickFile")}
            </Button>
          </>
        )}
        <Button
          type="button"
          size="xs"
          variant="outline"
          disabled={testing || !keyRef.trim()}
          onClick={() => void test()}
          title={t(`signing.testHint.${signing}`)}
        >
          {testing ? <LoaderCircle className="animate-spin" /> : <FlaskConical />}
          {t("signing.test")}
        </Button>
      </div>
      {result && (
        <p
          role="status"
          className={cn(
            "rounded-md border px-2 py-1 text-[11px] break-words",
            "error" in result || !result.ok
              ? "border-danger/40 bg-danger/10 text-danger"
              : "border-success/40 bg-success/10 text-success",
          )}
        >
          {"error" in result
            ? result.error
            : result.ok
              ? result.signature?.status === "verified"
                ? t("signing.testVerified")
                : t("signing.testSigned")
              : t(`signing.testFailed.${signing}`, { message: result.message ?? "" })}
        </p>
      )}
    </div>
  );
}
