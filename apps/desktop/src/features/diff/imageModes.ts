import type { BlobPreview } from "@/ipc";

export type ImageMode = "side" | "swipe" | "onion";

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|bmp|ico|svg|avif)$/i;

/** Whether the UI previews this path as an image. */
export function isImagePath(path: string): boolean {
  return IMAGE_EXT.test(path);
}

/** `data:` URL of a preview, or `null` when there is no content to show. */
export function dataUrl(p: BlobPreview | null | undefined): string | null {
  if (!p?.data) return null;
  return `data:${p.mime};base64,${p.data}`;
}

/** Modes usable for a pair: overlaying needs both sides. */
export function availableModes(
  original: BlobPreview | null | undefined,
  modified: BlobPreview | null | undefined,
): ImageMode[] {
  return dataUrl(original) && dataUrl(modified) ? ["side", "swipe", "onion"] : ["side"];
}

/** The mode to use: the requested one if available, else side by side. */
export function effectiveMode(
  wanted: ImageMode,
  original: BlobPreview | null | undefined,
  modified: BlobPreview | null | undefined,
): ImageMode {
  return availableModes(original, modified).includes(wanted) ? wanted : "side";
}

/** CSS `clip-path` showing the left `percent` of the top image (swipe). */
export function swipeClip(percent: number): string {
  const p = Math.min(100, Math.max(0, percent));
  return `inset(0 ${100 - p}% 0 0)`;
}

export const clampPercent = (v: number) => Math.min(100, Math.max(0, Math.round(v)));

/** `640 × 480`, or `null` while unknown. */
export function dimensions(size: { w: number; h: number } | null | undefined): string | null {
  return size && size.w > 0 ? `${size.w} × ${size.h}` : null;
}

/** Checkerboard background so transparent pixels are visible. */
export const CHECKERBOARD: React.CSSProperties = {
  backgroundImage:
    "conic-gradient(var(--surface-2) 25%, transparent 0 50%, var(--surface-2) 0 75%, transparent 0)",
  backgroundSize: "16px 16px",
};
