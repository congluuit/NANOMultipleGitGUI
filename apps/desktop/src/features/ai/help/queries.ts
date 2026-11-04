import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands } from "@/ipc";

export const chatKeys = { list: ["ai", "chats"] as const };

/** Stored "Hỏi AI" chats, newest first. */
export function useAiChats(enabled = true) {
  return useQuery({
    queryKey: chatKeys.list,
    queryFn: () => commands.aiChatList(),
    enabled: enabled && isTauri(),
  });
}
