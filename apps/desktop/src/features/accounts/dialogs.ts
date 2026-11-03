import { create } from "zustand";
import type { ProviderKind } from "@/ipc";

/** "Sign in to GitLab" for a remote's host: the provider the add-account wizard starts on. */
export interface SignInTarget {
  kind: ProviderKind;
  /** `https://gitlab.com`, or the self-hosted instance. */
  baseUrl: string;
}

/**
 * Open state of the account-related dialogs, so any feature (account pill, command palette,
 * welcome screen, sidebar) can open them without importing their components.
 */
interface AccountDialogs {
  /**
   * Account manager, optionally focused on one account, opened on the "add" flow (`signIn`:
   * already on that provider, the browser sign-in starting by itself), or straight on the
   * sign-in-again flow of `accountId` (`reauth`).
   */
  manager: {
    accountId: string | null;
    add: boolean;
    reauth: boolean;
    signIn: SignInTarget | null;
  } | null;
  /** Clone dialog, optionally preset to an account or a URL. */
  clone: { accountId: string | null; url: string | null } | null;
  /** SSH keys of an account. */
  sshKeys: { accountId: string } | null;
  /** First-run import of existing git/ssh/gh/glab configuration. */
  importWizard: boolean;
  openManager: (opts?: {
    accountId?: string | null;
    add?: boolean;
    reauth?: boolean;
    signIn?: SignInTarget | null;
  }) => void;
  openClone: (opts?: { accountId?: string | null; url?: string | null }) => void;
  openSshKeys: (accountId: string) => void;
  openImportWizard: () => void;
  close: (dialog: "manager" | "clone" | "sshKeys" | "importWizard") => void;
}

export const useAccountDialogs = create<AccountDialogs>()((set) => ({
  manager: null,
  clone: null,
  sshKeys: null,
  importWizard: false,
  openManager: (opts) =>
    set({
      manager: {
        accountId: opts?.accountId ?? null,
        add: Boolean(opts?.add || opts?.signIn),
        reauth: Boolean(opts?.reauth && opts.accountId),
        signIn: opts?.signIn ?? null,
      },
    }),
  openClone: (opts) =>
    set({ clone: { accountId: opts?.accountId ?? null, url: opts?.url ?? null } }),
  openSshKeys: (accountId) => set({ sshKeys: { accountId } }),
  openImportWizard: () => set({ importWizard: true }),
  close: (dialog) => set(dialog === "importWizard" ? { importWizard: false } : { [dialog]: null }),
}));
