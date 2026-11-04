import { useEffect, useRef, useState } from "react";
import { LoaderCircle, MessageSquareText, Send, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { relativeTime } from "@/features/clone/model";
import { type AiChat, commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { cn } from "@/lib/utils";
import { useSettings } from "@/stores/settings";
import { errorFix, isAiError, modelOf, newRequestId, sendsCodeAway, serviceName } from "../model";
import { useAiConfig, useAiSettings } from "../queries";
import { openAiSettings } from "../store";
import { aiErrorText } from "../suggest";
import { Answer } from "./Answer";
import { chatKeys, useAiChats } from "./queries";
import { type HelpRequest, rememberConsent, startHelpChat, useAiHelp } from "./store";

/** Mounted once in `App`: the consent question and the chat window. */
export function AiHelpHost() {
  // Keeps the AI settings cached: error notices offer "Hỏi AI" only when AI is set up.
  useAiSettings();
  const open = useAiHelp((s) => s.open);
  const consent = useAiHelp((s) => s.consent);
  return (
    <>
      {consent && <ConsentDialog req={consent} />}
      {open && <AiHelpDialog />}
    </>
  );
}

function ConsentDialog({ req }: { req: HelpRequest }) {
  const { t } = useTranslation();
  const config = useAiConfig();
  const [remember, setRemember] = useState(false);
  if (!config) return null;
  const service = serviceName(config);
  const away = sendsCodeAway(config);
  return (
    <ConfirmDialog
      open
      title={t("aiHelp.consent.title", { service })}
      description={t(away ? "aiHelp.consent.cloud" : "aiHelp.consent.local", {
        service,
        model: modelOf(config),
      })}
      confirmLabel={t("aiHelp.consent.confirm")}
      onCancel={() => useAiHelp.getState().setConsent(null)}
      onConfirm={() => {
        rememberConsent(remember);
        startHelpChat(req);
      }}
    >
      <div className="grid gap-2 text-xs">
        <span className="font-medium">{t("aiHelp.consent.preview")}</span>
        <pre className="max-h-56 overflow-auto rounded border bg-muted/50 p-2 font-mono text-[11px] break-all whitespace-pre-wrap">
          {req.context}
        </pre>
        <label className="flex cursor-pointer items-center gap-2">
          <input
            type="checkbox"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
          />
          {t("aiHelp.consent.remember")}
        </label>
      </div>
    </ConfirmDialog>
  );
}

interface Pending {
  requestId: string;
  message: string;
}

function AiHelpDialog() {
  const { t, i18n } = useTranslation();
  const config = useAiConfig();
  const chats = useAiChats().data ?? [];
  const activeId = useAiHelp((s) => s.activeId);
  const draft = useAiHelp((s) => s.draft);
  const active = chats.find((c) => c.id === activeId) ?? null;
  const [input, setInput] = useState("");
  const [pending, setPending] = useState<Pending | null>(null);
  const [failed, setFailed] = useState<{ message: string; error: unknown } | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  // Dates of the list are relative to when the window opened.
  const [now] = useState(() => Date.now());
  const sentDraft = useRef<HelpRequest | null>(null);
  const bottom = useRef<HTMLDivElement>(null);

  const send = async (message: string) => {
    const text = message.trim();
    if (!text || pending) return;
    const { activeId: chatId, draft: start } = useAiHelp.getState();
    if (!chatId && !start) return;
    const requestId = newRequestId("help");
    setPending({ requestId, message: text });
    setFailed(null);
    setInput("");
    try {
      const chat = await commands.aiChatSend(
        requestId,
        chatId,
        chatId ? null : start,
        text,
        useSettings.getState().language,
      );
      queryClient.setQueryData<AiChat[]>(chatKeys.list, (old = []) => [
        chat,
        ...old.filter((c) => c.id !== chat.id),
      ]);
      useAiHelp.getState().select(chat.id);
    } catch (e) {
      if (!(isAiError(e) && e.kind === "cancelled")) setFailed({ message: text, error: e });
      else setInput(text);
    } finally {
      setPending(null);
    }
  };

  // A chat started from an error asks its first question by itself.
  useEffect(() => {
    if (!draft || sentDraft.current === draft) return;
    sentDraft.current = draft;
    void send(t("aiHelp.firstQuestion"));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft]);

  useEffect(() => {
    bottom.current?.scrollIntoView?.({ block: "end" });
  }, [active?.messages.length, pending, failed]);

  const cancel = () => {
    if (pending) void commands.aiCancel(pending.requestId).catch(() => false);
  };

  const remove = async (id: string) => {
    const next = await commands.aiChatDelete(id);
    queryClient.setQueryData(chatKeys.list, next);
    if (id === useAiHelp.getState().activeId) useAiHelp.getState().select(null);
  };

  const clearAll = async () => {
    setConfirmClear(false);
    queryClient.setQueryData(chatKeys.list, await commands.aiChatDeleteAll());
    useAiHelp.getState().select(null);
  };

  const title = active?.title ?? draft?.title ?? t("aiHelp.title");
  const context = active?.context ?? draft?.context;
  const question = pending?.message ?? failed?.message;
  const canWrite = Boolean(active || draft);

  return (
    <Dialog open onOpenChange={(o) => !o && useAiHelp.getState().setOpen(false)}>
      <DialogContent className="h-[80vh] gap-0 overflow-hidden p-0 sm:max-w-4xl">
        <div className="flex min-h-0 flex-1">
          <aside className="flex w-56 shrink-0 flex-col border-r bg-muted/30">
            <div className="px-3 pt-4 pb-2 text-xs font-semibold">{t("aiHelp.history")}</div>
            <div className="min-h-0 flex-1 overflow-y-auto px-1.5">
              {chats.length === 0 && (
                <p className="px-1.5 text-[11px] text-muted-foreground">{t("aiHelp.noChats")}</p>
              )}
              {chats.map((c) => (
                <div
                  key={c.id}
                  className={cn(
                    "group flex items-start gap-1 rounded px-1.5 py-1 text-xs hover:bg-accent",
                    c.id === activeId && "bg-accent",
                  )}
                >
                  <button
                    type="button"
                    className="grid min-w-0 flex-1 text-left"
                    onClick={() => {
                      setFailed(null);
                      useAiHelp.getState().select(c.id);
                    }}
                  >
                    <span className="truncate">{c.title}</span>
                    <span className="truncate text-[10px] text-muted-foreground">
                      {[c.repoName, relativeTime(c.updatedAt / 1000, now, i18n.language)]
                        .filter(Boolean)
                        .join(" · ")}
                    </span>
                  </button>
                  <button
                    type="button"
                    aria-label={t("aiHelp.delete")}
                    title={t("aiHelp.delete")}
                    className="mt-0.5 opacity-0 group-hover:opacity-70 hover:opacity-100 focus:opacity-100"
                    onClick={() => void remove(c.id)}
                  >
                    <Trash2 className="size-3.5" />
                  </button>
                </div>
              ))}
            </div>
            <div className="grid gap-1 border-t p-2 text-[10px] text-muted-foreground">
              <span>{t("aiHelp.localOnly")}</span>
              <Button
                size="xs"
                variant="outline"
                disabled={chats.length === 0}
                onClick={() => setConfirmClear(true)}
              >
                {t("aiHelp.deleteAll")}
              </Button>
            </div>
          </aside>

          <section className="flex min-w-0 flex-1 flex-col">
            <DialogHeader className="border-b px-4 pt-4 pb-3 pr-10">
              <DialogTitle className="truncate text-sm">{title}</DialogTitle>
              <DialogDescription className="text-[11px]">
                {config
                  ? t("aiHelp.using", { service: serviceName(config), model: modelOf(config) })
                  : null}
              </DialogDescription>
            </DialogHeader>

            <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-4 py-3">
              {context && (
                <details className="text-[11px] text-muted-foreground">
                  <summary className="cursor-pointer select-none">{t("aiHelp.sent")}</summary>
                  <pre className="mt-1 max-h-48 overflow-auto rounded bg-muted/50 p-2 font-mono break-all whitespace-pre-wrap">
                    {context}
                  </pre>
                </details>
              )}
              {!canWrite && (
                <div className="grid place-items-center gap-2 pt-16 text-center text-xs text-muted-foreground">
                  <MessageSquareText className="size-6" />
                  <p className="max-w-xs">{t("aiHelp.empty")}</p>
                </div>
              )}
              {active?.messages.map((m, i) => (
                <Bubble key={i} role={m.role} text={m.content} />
              ))}
              {question && <Bubble role="user" text={question} />}
              {pending && (
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <LoaderCircle className="size-3.5 animate-spin" />
                  {t("aiHelp.thinking")}
                  <Button size="xs" variant="ghost" onClick={cancel}>
                    {t("common.cancel")}
                  </Button>
                </div>
              )}
              {failed && (
                <div role="alert" className="grid gap-2 rounded border border-destructive/40 p-2">
                  <span className="text-xs text-destructive">{aiErrorText(t, failed.error)}</span>
                  <span className="flex gap-1.5">
                    <Button size="xs" variant="outline" onClick={() => void send(failed.message)}>
                      {t("ai.retry")}
                    </Button>
                    {isAiError(failed.error) && errorFix(failed.error) === "settings" && (
                      <Button size="xs" variant="outline" onClick={openAiSettings}>
                        {t("ai.openSettings")}
                      </Button>
                    )}
                  </span>
                </div>
              )}
              <div ref={bottom} />
            </div>

            {canWrite && (
              <form
                className="flex items-end gap-2 border-t p-3"
                onSubmit={(e) => {
                  e.preventDefault();
                  void send(input);
                }}
              >
                <textarea
                  aria-label={t("aiHelp.placeholder")}
                  placeholder={t("aiHelp.placeholder")}
                  className="max-h-32 min-h-9 flex-1 resize-none rounded-md border bg-background px-2 py-1.5 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  rows={2}
                  value={input}
                  disabled={Boolean(pending)}
                  onChange={(e) => setInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                      e.preventDefault();
                      void send(input);
                    }
                  }}
                />
                <Button type="submit" size="sm" disabled={Boolean(pending) || !input.trim()}>
                  <Send className="size-3.5" />
                  {t("aiHelp.send")}
                </Button>
              </form>
            )}
          </section>
        </div>
        <ConfirmDialog
          open={confirmClear}
          title={t("aiHelp.deleteAllTitle")}
          description={t("aiHelp.deleteAllText", { count: chats.length })}
          confirmLabel={t("aiHelp.deleteAll")}
          destructive
          onCancel={() => setConfirmClear(false)}
          onConfirm={() => void clearAll()}
        />
      </DialogContent>
    </Dialog>
  );
}

function Bubble({ role, text }: { role: "user" | "assistant"; text: string }) {
  return role === "user" ? (
    <div className="ml-auto w-fit max-w-[85%] rounded-lg bg-primary px-3 py-1.5 text-xs whitespace-pre-wrap text-primary-foreground">
      {text}
    </div>
  ) : (
    <div className="max-w-[95%] rounded-lg border bg-muted/30 px-3 py-2">
      <Answer text={text} />
    </div>
  );
}
