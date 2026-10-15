import { useState } from "react";
import { Tabs } from "radix-ui";
import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { Account } from "@/ipc";
import { cn } from "@/lib/utils";
import { useAccountDialogs } from "../dialogs";
import { useAccounts, useIdentities, useProviders, useRules } from "../queries";
import { AccountsTab } from "./AccountsTab";
import { AddAccountWizard } from "./AddAccountWizard";
import { ProvidersTab } from "./ProvidersTab";
import { useAccountHealth } from "./queries";
import { RulesTab } from "./RulesTab";
import { SettingsTab } from "./SettingsTab";

type Tab = "accounts" | "rules" | "providers" | "settings";

/** Mounted once (App.tsx); opened with `useAccountDialogs().openManager(...)`. */
export function AccountManagerHost() {
  const state = useAccountDialogs((s) => s.manager);
  const close = useAccountDialogs((s) => s.close);
  const { t } = useTranslation();
  return (
    <Dialog open={state !== null} onOpenChange={(o) => !o && close("manager")}>
      <DialogContent className="flex h-[min(88vh,760px)] flex-col gap-3 sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t("accounts.manager.title")}</DialogTitle>
          <DialogDescription>{t("accounts.manager.description")}</DialogDescription>
        </DialogHeader>
        {state && (
          <AccountManager
            // A fresh view (and wizard) each time the dialog is opened.
            key={`${state.accountId ?? ""}:${state.add}:${state.reauth}`}
            focusAccountId={state.accountId}
            startWithAdd={state.add}
            startWithReauth={state.reauth}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

const TABS: Tab[] = ["accounts", "rules", "providers", "settings"];

export function AccountManager({
  focusAccountId,
  startWithAdd,
  startWithReauth = false,
}: {
  focusAccountId: string | null;
  startWithAdd: boolean;
  startWithReauth?: boolean;
}) {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const providers = useProviders();
  const rules = useRules();
  const identities = useIdentities(null);
  const health = useAccountHealth();
  const [tab, setTab] = useState<Tab>("accounts");
  const [wizardState, setWizard] = useState<{ reauth: Account | null } | null>(
    startWithAdd ? { reauth: null } : null,
  );
  const [reauthPending, setReauthPending] = useState(startWithReauth);
  const reauthTarget = reauthPending
    ? (accounts.data?.find((a) => a.id === focusAccountId) ?? null)
    : null;
  // Opened from an error ("sign in again"): show that account's wizard once the list loads.
  const wizard = wizardState ?? (reauthTarget ? { reauth: reauthTarget } : null);
  const closeWizard = () => {
    setReauthPending(false);
    setWizard(null);
  };
  const [focus, setFocus] = useState<string | null>(focusAccountId);
  const [providerFocus, setProviderFocus] = useState<string | null>(null);

  return (
    <Tabs.Root
      value={tab}
      onValueChange={(v) => setTab(v as Tab)}
      className="flex min-h-0 flex-1 flex-col gap-3"
    >
      <Tabs.List aria-label={t("accounts.manager.title")} className="flex gap-1 border-b">
        {TABS.map((v) => (
          <Tabs.Trigger
            key={v}
            value={v}
            className={cn(
              "-mb-px border-b-2 border-transparent px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground",
              "data-[state=active]:border-primary data-[state=active]:font-medium data-[state=active]:text-foreground",
            )}
          >
            {t(`accounts.tab.${v}`)}
          </Tabs.Trigger>
        ))}
      </Tabs.List>
      <div className="min-h-0 flex-1 overflow-y-auto pr-1">
        <Tabs.Content value="accounts">
          {wizard ? (
            <AddAccountWizard
              providers={providers.data ?? []}
              reauth={wizard.reauth}
              onCancel={closeWizard}
              onDone={(account) => {
                closeWizard();
                setFocus(account.id);
              }}
              onEditProvider={(id) => {
                closeWizard();
                setProviderFocus(id);
                setTab("providers");
              }}
            />
          ) : (
            <AccountsTab
              key={focus ?? ""}
              accounts={accounts.data ?? []}
              providers={providers.data ?? []}
              health={health.data ?? []}
              focusId={focus}
              onAdd={() => {
                setReauthPending(false);
                setWizard({ reauth: null });
              }}
              onReauth={(account) => setWizard({ reauth: account })}
            />
          )}
        </Tabs.Content>
        <Tabs.Content value="rules">
          <RulesTab
            rules={rules.data ?? []}
            accounts={accounts.data ?? []}
            identities={identities.data ?? []}
          />
        </Tabs.Content>
        <Tabs.Content value="providers">
          <ProvidersTab
            key={providerFocus ?? ""}
            providers={providers.data ?? []}
            focusId={providerFocus}
          />
        </Tabs.Content>
        <Tabs.Content value="settings">
          <SettingsTab />
        </Tabs.Content>
      </div>
    </Tabs.Root>
  );
}
