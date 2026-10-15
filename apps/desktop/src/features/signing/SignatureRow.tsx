import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { LoaderCircle, ShieldAlert, ShieldCheck, ShieldQuestion } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands, type SignatureKind } from "@/ipc";
import { cn } from "@/lib/utils";
import { signatureTone, signerText } from "./model";

const TONE = {
  ok: "border-success/40 text-success",
  warning: "border-warning/50 text-warning",
  danger: "border-danger/50 text-danger",
  muted: "border-border text-muted-foreground",
} as const;

/**
 * "Chữ ký" row of the commit details: verified lazily (gpg / ssh-keygen run only for signed
 * commits; SSH signatures are checked against the app's allowed signers from identities).
 */
export function SignatureRow({
  repoId,
  hash,
  kind,
}: {
  repoId: number;
  hash: string;
  kind: SignatureKind;
}) {
  const { t } = useTranslation();
  const sig = useQuery({
    queryKey: ["commitSignature", repoId, hash],
    queryFn: () => commands.commitSignature(repoId, hash),
    enabled: isTauri() && kind !== "none",
    staleTime: 5 * 60_000,
    retry: false,
  });
  if (kind === "none") return null;
  const status = sig.data?.status;
  const tone = status ? signatureTone(status) : "muted";
  const Icon = tone === "ok" ? ShieldCheck : tone === "danger" ? ShieldAlert : ShieldQuestion;
  return (
    <div className="grid grid-cols-[84px_1fr] items-center gap-2">
      <span className="text-muted-foreground">{t("signing.row")}</span>
      <span className="flex min-w-0 items-center gap-1.5">
        {sig.isPending ? (
          <LoaderCircle className="size-3.5 animate-spin text-muted-foreground" />
        ) : (
          <span
            className={cn(
              "inline-flex items-center gap-1 rounded-full border px-1.5 py-px text-[10px] font-medium",
              TONE[tone],
            )}
            title={
              sig.data
                ? [
                    t(`signing.statusHint.${sig.data.status}`),
                    signerText(sig.data),
                    sig.data.message ?? "",
                  ]
                    .filter(Boolean)
                    .join("\n")
                : undefined
            }
          >
            <Icon className="size-3" />
            {status ? t(`signing.status.${status}`) : t("signing.status.unknownKey")}
          </span>
        )}
        <span className="truncate text-[11px] text-muted-foreground">
          {t(`signing.kind.${kind}`)}
          {sig.data?.signer ? ` · ${sig.data.signer}` : ""}
        </span>
      </span>
    </div>
  );
}
