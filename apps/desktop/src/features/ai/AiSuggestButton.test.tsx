import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { type AiSettingsView, commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { EMPTY_DRAFT, useWorktreeStore } from "@/stores/worktree";
import { AiSuggestButton } from "./AiSuggestButton";
import { DEFAULT_CONFIG } from "./model";
import { aiKeys } from "./queries";
import { useAiUi } from "./store";

const view = (enabled: boolean): AiSettingsView => ({
  config: { ...DEFAULT_CONFIG, enabled },
  keys: { openai: false, anthropic: false, gemini: false, compatible: false },
});

const renderButton = () =>
  render(
    <QueryClientProvider client={queryClient}>
      <AiSuggestButton repoId={3} />
    </QueryClientProvider>,
  );

const draft = () => useWorktreeStore.getState().drafts[3] ?? EMPTY_DRAFT;

describe("AiSuggestButton", () => {
  beforeAll(() => {
    initI18n("vi");
  });

  beforeEach(() => {
    vi.restoreAllMocks();
    useWorktreeStore.setState({ drafts: {} });
    useAiUi.setState({ undo: {}, generating: {} });
    useUi.setState({ notice: null });
  });

  it("is hidden while AI suggestions are off", () => {
    queryClient.setQueryData(aiKeys.settings, view(false));
    renderButton();
    expect(screen.queryByTestId("ai-suggest")).toBeNull();
  });

  it("fills an empty draft and offers undo", async () => {
    queryClient.setQueryData(aiKeys.settings, view(true));
    const gen = vi.spyOn(commands, "aiGenerate").mockResolvedValue({
      summary: "feat: add login page",
      body: "- Add form",
      source: "staged",
      truncated: false,
      provider: "ollama",
      model: "qwen2.5-coder:1.5b",
    });
    renderButton();
    await userEvent.click(screen.getByRole("button", { name: "Gợi ý commit message bằng AI" }));
    await waitFor(() => expect(draft().summary).toBe("feat: add login page"));
    expect(draft().body).toBe("- Add form");
    expect(gen).toHaveBeenCalledWith(3, expect.stringMatching(/^ai-/), expect.any(String), false);

    await userEvent.click(screen.getByRole("button", { name: /^Hoàn tác/ }));
    expect(draft().summary).toBe("");
  });

  it("asks before replacing typed text", async () => {
    queryClient.setQueryData(aiKeys.settings, view(true));
    useWorktreeStore.getState().updateDraft(3, { summary: "my text" });
    vi.spyOn(commands, "aiGenerate").mockResolvedValue({
      summary: "fix: handle empty input",
      body: "",
      source: "staged",
      truncated: false,
      provider: "ollama",
      model: "m",
    });
    renderButton();
    await userEvent.click(screen.getByRole("button", { name: "Gợi ý commit message bằng AI" }));
    await screen.findByText("Thay nội dung đang viết?");
    expect(draft().summary).toBe("my text");
    await userEvent.click(screen.getByRole("button", { name: "Thay bằng gợi ý" }));
    expect(draft().summary).toBe("fix: handle empty input");
  });

  it("explains a missing stage with a quick action", async () => {
    queryClient.setQueryData(aiKeys.settings, view(true));
    vi.spyOn(commands, "aiGenerate").mockRejectedValue({
      kind: "nothingStaged",
      message: "nothing is staged",
      hasUnstaged: true,
    });
    renderButton();
    await userEvent.click(screen.getByRole("button", { name: "Gợi ý commit message bằng AI" }));
    await waitFor(() => expect(useUi.getState().notice).not.toBeNull());
    const n = useUi.getState().notice;
    expect(n?.message).toContain("Hãy stage thay đổi trước");
    expect(n?.actions?.[0]?.label).toBe("Dùng thay đổi chưa stage");
  });
});
