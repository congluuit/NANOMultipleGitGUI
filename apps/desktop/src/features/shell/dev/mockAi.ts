/**
 * Dev-only fake AI for the web mock, added with `&ai`: Ollama is set up, "Hỏi AI" appears on
 * error notices and answers after a moment. Chats live in memory (gone on reload).
 */
import type { AiChat, AiChatMessage } from "@/ipc";

type Handler = (args: Record<string, unknown>) => unknown;

const ANSWER = [
  "Remote **origin** đang gắn với tài khoản GitLab, nên app không gửi thông tin đăng nhập tới github.com.",
  "",
  "1. Bấm nút `Dùng Pret cho origin` trên thông báo lỗi.",
  "2. Bấm Push lại.",
  "",
  "Nếu muốn làm bằng lệnh:",
  "```",
  "git push origin main",
  "```",
].join(String.fromCharCode(10));

let chats: AiChat[] = [];

export function aiHandlers(): Record<string, Handler> {
  return {
    ai_settings: () => ({
      config: { enabled: true, provider: "ollama", models: { ollama: "qwen2.5-coder:3b" } },
      keys: { openai: false, anthropic: false, gemini: false, compatible: false },
    }),
    ai_chat_list: () => chats,
    ai_chat_send: (a) =>
      new Promise((resolve) =>
        setTimeout(() => {
          const now = Date.now();
          const start = a.start as { title: string; context: string; repoName: string | null };
          const old = chats.find((c) => c.id === a.chatId);
          const messages: AiChatMessage[] = [
            ...(old?.messages ?? []),
            { role: "user", content: a.message as string, at: now },
            {
              role: "assistant",
              content: old ? "Đúng vậy, cứ làm theo các bước trên." : ANSWER,
              at: now,
            },
          ];
          const chat: AiChat = old
            ? { ...old, messages, updatedAt: now }
            : {
                id: `chat-${now}`,
                title: start.title,
                context: start.context,
                repoName: start.repoName,
                provider: "ollama",
                model: "qwen2.5-coder:3b",
                createdAt: now,
                updatedAt: now,
                messages,
              };
          chats = [chat, ...chats.filter((c) => c.id !== chat.id)];
          resolve(chat);
        }, 1200),
      ),
    ai_chat_delete: (a) => (chats = chats.filter((c) => c.id !== a.id)),
    ai_chat_delete_all: () => (chats = []),
  };
}
