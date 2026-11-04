import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { type AiChat, type AiSettingsView, commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { DEFAULT_CONFIG } from "../model";
import { aiKeys } from "../queries";
import { AiHelpHost } from "./AiHelpDialog";
import { chatKeys } from "./queries";
import { Answer } from "./Answer";
import { askAi, useAiHelp } from "./store";

const SETTINGS: AiSettingsView = {
  config: { ...DEFAULT_CONFIG, enabled: true, provider: "openai" },
  keys: { openai: true, anthropic: false, gemini: false, compatible: false },
};

const REQUEST = { title: "Push bị từ chối", context: "Error: Push bị từ chối", repoName: "api" };

const chat = (messages: [string, string][]): AiChat => ({
  id: "chat-1",
  title: REQUEST.title,
  context: REQUEST.context,
  repoName: "api",
  provider: "openai",
  model: "gpt-4.1-mini",
  createdAt: 1,
  updatedAt: 1,
  messages: messages.flatMap(([q, a]) => [
    { role: "user" as const, content: q, at: 1 },
    { role: "assistant" as const, content: a, at: 2 },
  ]),
});

const renderHost = () =>
  render(
    <QueryClientProvider client={queryClient}>
      <AiHelpHost />
    </QueryClientProvider>,
  );

describe("Hỏi AI", () => {
  beforeAll(() => {
    initI18n("vi");
  });

  beforeEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
    queryClient.setQueryData(aiKeys.settings, SETTINGS);
    queryClient.setQueryData(chatKeys.list, []);
    useAiHelp.setState({ open: false, activeId: null, draft: null, consent: null });
  });

  it("asks before sending, then sends the first question by itself", async () => {
    const user = userEvent.setup();
    const send = vi
      .spyOn(commands, "aiChatSend")
      .mockResolvedValue(chat([["Q", "Bấm **Pull** rồi `Push` lại."]]));
    renderHost();
    askAi(REQUEST);
    expect(await screen.findByText("Gửi lỗi này cho OpenAI để hỏi cách sửa?")).toBeInTheDocument();
    expect(screen.getByText("Error: Push bị từ chối", { selector: "pre" })).toBeInTheDocument();
    expect(send).not.toHaveBeenCalled();

    await user.click(screen.getByRole("checkbox"));
    await user.click(screen.getByRole("button", { name: "Gửi và hỏi AI" }));
    await waitFor(() => expect(send).toHaveBeenCalledTimes(1));
    expect(send.mock.calls[0]?.[1]).toBeNull();
    expect(send.mock.calls[0]?.[2]).toEqual(REQUEST);
    expect(send.mock.calls[0]?.[3]).toMatch(/Lỗi này nghĩa là gì/);
    expect(await screen.findByText("Pull")).toBeInTheDocument();
    expect(useAiHelp.getState().activeId).toBe("chat-1");

    // "Không hỏi lại": the next error opens the chat directly.
    useAiHelp.setState({ open: false });
    askAi(REQUEST);
    expect(useAiHelp.getState().consent).toBeNull();
    expect(useAiHelp.getState().open).toBe(true);
  });

  it("continues a chat, keeps the question on failure and retries it", async () => {
    const user = userEvent.setup();
    queryClient.setQueryData(chatKeys.list, [chat([["Q1", "A1"]])]);
    useAiHelp.setState({ open: true, activeId: "chat-1" });
    const send = vi
      .spyOn(commands, "aiChatSend")
      .mockRejectedValueOnce({ kind: "network", message: "offline" })
      .mockResolvedValueOnce(
        chat([
          ["Q1", "A1"],
          ["Còn cách khác?", "A2"],
        ]),
      );
    renderHost();
    await user.type(screen.getByRole("textbox"), "Còn cách khác?{Enter}");
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.getByText("Còn cách khác?")).toBeInTheDocument();
    expect(send.mock.calls[0]?.[1]).toBe("chat-1");
    expect(send.mock.calls[0]?.[2]).toBeNull();

    await user.click(screen.getByRole("button", { name: "Thử lại" }));
    expect(await screen.findByText("A2")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("deletes one chat or the whole history", async () => {
    const user = userEvent.setup();
    queryClient.setQueryData(chatKeys.list, [chat([["Q1", "A1"]])]);
    useAiHelp.setState({ open: true, activeId: "chat-1" });
    const del = vi.spyOn(commands, "aiChatDelete").mockResolvedValue([]);
    const all = vi.spyOn(commands, "aiChatDeleteAll").mockResolvedValue([]);
    renderHost();
    await user.click(screen.getByRole("button", { name: "Xóa cuộc trò chuyện" }));
    expect(del).toHaveBeenCalledWith("chat-1");
    expect(await screen.findByText("Chưa có cuộc trò chuyện nào.")).toBeInTheDocument();
    expect(useAiHelp.getState().activeId).toBeNull();

    queryClient.setQueryData(chatKeys.list, [chat([["Q1", "A1"]])]);
    await user.click(await screen.findByRole("button", { name: "Xóa toàn bộ lịch sử" }));
    const confirm = await screen.findAllByRole("button", { name: "Xóa toàn bộ lịch sử" });
    await user.click(confirm[confirm.length - 1]!);
    expect(all).toHaveBeenCalled();
  });
});

describe("Answer", () => {
  it("shows code blocks, inline code and bold as text, never as HTML", () => {
    const { container } = render(
      <Answer text={"1. Chạy:\n```bash\ngit push -u origin main\n```\n**Lưu ý** `<b>x</b>`"} />,
    );
    expect(container.querySelector("pre")?.textContent).toBe("git push -u origin main");
    expect(container.querySelector("strong")?.textContent).toBe("Lưu ý");
    expect(container.querySelector("code")?.textContent).toBe("<b>x</b>");
    expect(container.querySelector("b")).toBeNull();
  });
});
