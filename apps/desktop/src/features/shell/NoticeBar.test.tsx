import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { useUi } from "@/stores/ui";
import { NOTICE_MS, NoticeBar } from "./Banners";

initI18n("en");

describe("NoticeBar", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.useRealTimers();
    useUi.getState().showNotice(null);
  });

  it("counts down and hides every kind of notice after 5 seconds", () => {
    act(() => useUi.getState().showNotice({ kind: "error", message: "Push failed" }));
    render(<NoticeBar />);
    expect(screen.getByText("5s")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(2_000));
    expect(screen.getByText("3s")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(NOTICE_MS - 2_000));
    expect(useUi.getState().notice).toBeNull();
  });

  it("pauses while the pointer is on it", () => {
    act(() => useUi.getState().showNotice({ kind: "success", message: "Switched" }));
    render(<NoticeBar />);
    fireEvent.pointerEnter(screen.getByRole("status"));
    act(() => vi.advanceTimersByTime(NOTICE_MS * 2));
    expect(useUi.getState().notice).not.toBeNull();
    fireEvent.pointerLeave(screen.getByRole("status"));
    act(() => vi.advanceTimersByTime(NOTICE_MS));
    expect(useUi.getState().notice).toBeNull();
  });

  it("restarts the countdown for a new notice", () => {
    act(() => useUi.getState().showNotice({ kind: "info", message: "A" }));
    render(<NoticeBar />);
    act(() => vi.advanceTimersByTime(4_000));
    act(() => useUi.getState().showNotice({ kind: "info", message: "A" }));
    expect(screen.getByText("5s")).toBeInTheDocument();
  });
});
