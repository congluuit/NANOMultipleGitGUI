import { useState } from "react";
import { cn } from "@/lib/utils";
import { accountLabel, initialOf } from "./switcher/model";

/** Anything with a color and a name: a real `Account`, an `AccountSummary`… */
export interface AvatarAccount {
  color: string;
  label?: string | null;
  displayName?: string | null;
  username?: string;
  avatarUrl?: string | null;
}

const nameOf = (a: AvatarAccount) =>
  accountLabel({
    label: a.label ?? null,
    displayName: a.displayName ?? null,
    username: a.username ?? "",
  });

/**
 * Round avatar of an account: its provider avatar (`avatarUrl`), or the initial on the account
 * color when there is none or it fails to load. `null` → dashed "?" (unbound).
 */
export function AccountAvatar({
  account,
  size = 20,
  className,
}: {
  account: AvatarAccount | null;
  size?: number;
  className?: string;
}) {
  const [failed, setFailed] = useState<string | null>(null);
  const url = account?.avatarUrl ?? null;
  const showImage = url !== null && failed !== url;
  return (
    <span
      aria-hidden
      className={cn(
        "relative inline-flex shrink-0 items-center justify-center overflow-hidden rounded-full font-semibold text-white",
        !account &&
          "border border-dashed border-muted-foreground bg-transparent text-muted-foreground",
        className,
      )}
      style={{
        width: size,
        height: size,
        fontSize: Math.round(size * 0.5),
        background: account?.color,
        boxShadow: showImage && account ? `0 0 0 1.5px ${account.color}` : undefined,
      }}
    >
      {account ? initialOf(nameOf(account)) : "?"}
      {showImage && (
        <img
          src={url}
          alt=""
          draggable={false}
          referrerPolicy="no-referrer"
          onError={() => setFailed(url)}
          className="absolute inset-0 size-full object-cover"
        />
      )}
    </span>
  );
}
