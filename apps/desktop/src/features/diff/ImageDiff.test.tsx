import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { initI18n } from "@/i18n";
import type { BlobPreview, DiffRequest, ImagePair } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { ImageDiff } from "./ImageDiff";
import {
  availableModes,
  clampPercent,
  dataUrl,
  dimensions,
  effectiveMode,
  isImagePath,
  swipeClip,
} from "./imageModes";

const png = (data: string | null, extra: Partial<BlobPreview> = {}): BlobPreview => ({
  size: 1234,
  mime: "image/png",
  data,
  lfs: null,
  lfsMissing: false,
  tooLarge: false,
  binary: true,
  ...extra,
});

describe("image mode helpers", () => {
  it("recognises image paths", () => {
    expect(isImagePath("a/b.PNG")).toBe(true);
    expect(isImagePath("icon.svg")).toBe(true);
    expect(isImagePath("x.jpeg")).toBe(true);
    expect(isImagePath("notes.txt")).toBe(false);
  });

  it("offers overlay modes only when both sides have pixels", () => {
    const a = png("AAAA");
    expect(availableModes(a, a)).toEqual(["side", "swipe", "onion"]);
    expect(availableModes(null, a)).toEqual(["side"]);
    expect(availableModes(png(null, { lfsMissing: true }), a)).toEqual(["side"]);
    expect(effectiveMode("onion", null, a)).toBe("side");
    expect(effectiveMode("swipe", a, a)).toBe("swipe");
  });

  it("builds data URLs, clip paths and sizes", () => {
    expect(dataUrl(png("QUJD"))).toBe("data:image/png;base64,QUJD");
    expect(dataUrl(png(null))).toBeNull();
    expect(swipeClip(30)).toBe("inset(0 70% 0 0)");
    expect(swipeClip(150)).toBe("inset(0 0% 0 0)");
    expect(clampPercent(-4)).toBe(0);
    expect(clampPercent(42.6)).toBe(43);
    expect(dimensions({ w: 640, h: 480 })).toBe("640 × 480");
    expect(dimensions(null)).toBeNull();
  });
});

const REQ: DiffRequest = { side: "commit", path: "img/logo.png", commit: "abc1234" };

function renderDiff(pair: ImagePair) {
  queryClient.setQueryData(["contents", 1, "images", REQ.side, REQ.path, null, REQ.commit], pair);
  return render(
    <QueryClientProvider client={queryClient}>
      <ImageDiff repoId={1} request={REQ} />
    </QueryClientProvider>,
  );
}

describe("ImageDiff", () => {
  beforeAll(() => {
    initI18n("vi");
  });
  beforeEach(() => queryClient.clear());

  it("switches between side by side, swipe and onion skin", async () => {
    const user = userEvent.setup();
    renderDiff({ original: png("T0xE"), modified: png("TkVX", { size: 2048 }) });
    expect(screen.getByAltText("Trước")).toHaveAttribute("src", "data:image/png;base64,T0xE");
    expect(screen.getByText("2 KB")).toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: "Trượt" }));
    const top = screen.getByTestId("swipe-top");
    expect(top.style.clipPath).toBe("inset(0 50% 0 0)");
    fireEvent.change(screen.getByLabelText("Vị trí"), { target: { value: "20" } });
    expect(screen.getByTestId("swipe-top").style.clipPath).toBe("inset(0 80% 0 0)");

    await user.click(screen.getByRole("radio", { name: "Chồng mờ" }));
    fireEvent.change(screen.getByLabelText("Độ hiện bản sau"), { target: { value: "75" } });
    expect(screen.getByTestId("onion-top").style.opacity).toBe("0.25");
    expect(screen.getByText("75%")).toBeInTheDocument();
  });

  it("shows a new file side by side only, and explains a missing LFS object", () => {
    const oid = "f".repeat(64);
    renderDiff({
      original: null,
      modified: png(null, { lfs: { oid, size: 5_000_000 }, lfsMissing: true, size: 5_000_000 }),
    });
    expect(screen.getByRole("radio", { name: "Trượt" })).toBeDisabled();
    expect(screen.getByText("Không có (file mới)")).toBeInTheDocument();
    expect(screen.getByText(/chưa được tải về máy/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Tải file này/ })).toBeInTheDocument();
    expect(screen.getByText(/LFS · oid ffffffffffff… · 4.77 MB/)).toBeInTheDocument();
  });
});
