import { create } from "zustand";
import type { HelpAction } from "./actions";

/** What a new chat is about: built from an error notice by `helpRequest`. */
export interface HelpRequest {
  title: string;
  /** Exactly what is sent to the AI service (shown before asking for consent). */
  context: string;
  repoName: string | null;
  /** Folder of that repository ("" when the error was outside one). */
  repoPath?: string;
  /** What the app can run for this error (offered as buttons under the answers). */
  actions?: HelpAction[];
}

interface AiHelpState {
  /** The chat window is open. */
  open: boolean;
  /** Chat shown in the window; `null` with `draft` set = a new chat about to be sent. */
  activeId: string | null;
  /** A new chat waiting for its first answer. */
  draft: HelpRequest | null;
  /** Asking the user before anything is sent. */
  consent: HelpRequest | null;
  setOpen: (open: boolean) => void;
  select: (id: string | null) => void;
  setDraft: (draft: HelpRequest | null) => void;
  setConsent: (consent: HelpRequest | null) => void;
}

export const useAiHelp = create<AiHelpState>()((set) => ({
  open: false,
  activeId: null,
  draft: null,
  consent: null,
  setOpen: (open) => set({ open }),
  select: (activeId) => set({ activeId, draft: null }),
  setDraft: (draft) => set({ draft }),
  setConsent: (consent) => set({ consent }),
}));

const NO_ASK_KEY = "nanomgg.aiHelp.noAsk";

/** The user ticked "Không hỏi lại" in the consent dialog. */
export function consentRemembered(): boolean {
  try {
    return localStorage.getItem(NO_ASK_KEY) === "1";
  } catch {
    return false;
  }
}

export function rememberConsent(on: boolean) {
  try {
    if (on) localStorage.setItem(NO_ASK_KEY, "1");
    else localStorage.removeItem(NO_ASK_KEY);
  } catch {
    // Private storage off: the dialog simply asks again next time.
  }
}

/** Opens the window on a new chat about `req` (the first question is sent by the window). */
export function startHelpChat(req: HelpRequest) {
  useAiHelp.setState({ open: true, activeId: null, draft: req, consent: null });
}

/** "Hỏi AI": asks for consent first unless the user said not to. */
export function askAi(req: HelpRequest) {
  if (consentRemembered()) startHelpChat(req);
  else useAiHelp.getState().setConsent(req);
}

/** Opens the chat history (Settings, command palette). */
export function openAiHelp() {
  useAiHelp.setState({ open: true, draft: null });
}
