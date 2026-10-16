import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { useUi } from "@/stores/ui";
import { NoticeBar } from "./Banners";
import { NOTICE_LONG_MS, NOTICE_SHORT_MS, noticeDuration } from "./noticeDuration";

initI18n("en");

describe("noticeDuration", () => {
  it("keeps errors and notices with something to read for 5 seconds, the rest for 3", () => {
    expect(noticeDuration({ kind: "error", message: "Push failed" })).toBe(NOTICE_LONG_MS);
    expect(noticeDuration({ kind: "success", message: "Switched" })).toBe(NOTICE_SHORT_MS);
    expect(noticeDuration({ kind: "info", message: "Up to date" })).toBe(NOTICE_SHORT_MS);
    expect(noticeDuration({ kind: "info", message: "A", hint: "Do this" })).toBe(NOTICE_LONG_MS);
    expect(noticeDuration({ kind: "success", message: "A", detail: "log" })).toBe(NOTICE_LONG_MS);
    expect(
      noticeDuration({ kind: "info", message: "A", actions: [{ label: "Open", run: () => {} }] }),
    ).toBe(NOTICE_LONG_MS);
  });
});

describe("NoticeBar", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.useRealTimers();
    useUi.getState().showNotice(null);
  });

  it("counts down and hides an error after 5 seconds", () => {
    act(() => useUi.getState().showNotice({ kind: "error", message: "Push failed" }));
    render(<NoticeBar />);
    expect(screen.getByText("5s")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(2_000));
    expect(screen.getByText("3s")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(NOTICE_LONG_MS - 2_000));
    expect(useUi.getState().notice).toBeNull();
  });

  it("hides a plain success after 3 seconds", () => {
    act(() => useUi.getState().showNotice({ kind: "success", message: "Switched" }));
    render(<NoticeBar />);
    expect(screen.getByText("3s")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(NOTICE_SHORT_MS - 100));
    expect(useUi.getState().notice).not.toBeNull();
    act(() => vi.advanceTimersByTime(100));
    expect(useUi.getState().notice).toBeNull();
  });

  it("pauses while the pointer is on it", () => {
    act(() => useUi.getState().showNotice({ kind: "success", message: "Switched" }));
    render(<NoticeBar />);
    fireEvent.pointerEnter(screen.getByRole("status"));
    act(() => vi.advanceTimersByTime(NOTICE_LONG_MS * 2));
    expect(useUi.getState().notice).not.toBeNull();
    fireEvent.pointerLeave(screen.getByRole("status"));
    act(() => vi.advanceTimersByTime(NOTICE_SHORT_MS));
    expect(useUi.getState().notice).toBeNull();
  });

  it("restarts the countdown for a new notice", () => {
    act(() => useUi.getState().showNotice({ kind: "info", message: "A" }));
    render(<NoticeBar />);
    act(() => vi.advanceTimersByTime(2_000));
    act(() => useUi.getState().showNotice({ kind: "info", message: "A" }));
    expect(screen.getByText("3s")).toBeInTheDocument();
  });
});
