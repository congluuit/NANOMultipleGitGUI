import { ChevronDown } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { AccountAvatar } from "@/features/accounts/AccountAvatar";
import { accountLabel, useSortedAccounts } from "@/features/accounts/switcher";
import type { Account } from "@/ipc";

/** Colored dot + account label. */
export function AccountDot({ account }: { account: Account | null }) {
  if (!account) return null;
  return (
    <span className="inline-flex min-w-0 items-center gap-1">
      <span
        aria-hidden
        className="size-2 shrink-0 rounded-full"
        style={{ background: account.color }}
      />
      <span className="truncate">{accountLabel(account)}</span>
    </span>
  );
}

/**
 * "As ● account ▾" shown next to every write button (PLAN §4.5): the account that will act,
 * changeable for this action only among the accounts of the same provider that have a token.
 */
export function ActingAs({
  accountId,
  onChange,
  disabled,
}: {
  accountId: string;
  onChange: (accountId: string) => void;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const accounts = useSortedAccounts();
  const current = accounts.find((a) => a.id === accountId) ?? null;
  const choices = accounts.filter(
    (a) => a.secrets.includes("token") && (!current || a.providerId === current.providerId),
  );
  const label = current ? accountLabel(current) : accountId;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        disabled={disabled || choices.length < 2}
        className="inline-flex max-w-full min-w-0 items-center gap-1 rounded px-1 text-[11px] text-muted-foreground hover:bg-surface-2 hover:text-foreground disabled:hover:bg-transparent"
        title={t("issues.actingAsHint")}
      >
        <span className="shrink-0">{t("issues.actingAs")}</span>
        {current && (
          <span
            aria-hidden
            className="size-2 shrink-0 rounded-full"
            style={{ background: current.color }}
          />
        )}
        <span className="truncate font-medium text-foreground">{label}</span>
        {choices.length >= 2 && <ChevronDown className="size-3 shrink-0" />}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-60 text-xs">
        <DropdownMenuLabel className="text-[11px] text-muted-foreground">
          {t("issues.actingAsOnce")}
        </DropdownMenuLabel>
        {choices.map((a) => (
          <DropdownMenuItem key={a.id} onSelect={() => onChange(a.id)}>
            <AccountAvatar account={a} size={16} />
            <span className="truncate">{accountLabel(a)}</span>
            {a.id === accountId && <span className="ml-auto text-muted-foreground">✓</span>}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
