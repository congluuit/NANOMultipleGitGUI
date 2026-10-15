/**
 * CI badge for the commit graph canvas (Phase 4.3). The graph calls {@link drawCiBadge} for rows
 * that have a CI state (see `useCiStatuses`); colours come from {@link readCiColors} (theme tokens,
 * re-read when the theme changes, like the graph palette).
 */
import type { CiState } from "@/ipc";
import { readToken } from "@/theme/theme";
import { ciGlyph, ciToken } from "./model";

export interface CiColors {
  success: string;
  danger: string;
  warning: string;
  muted: string;
  /** Background behind the badge (row / canvas background). */
  bg: string;
  font: string;
}

/** Diameter of the badge in CSS pixels. */
export const CI_BADGE_SIZE = 14;

export function readCiColors(el: Element = document.documentElement): CiColors {
  return {
    success: readToken("--success", el),
    danger: readToken("--danger", el),
    warning: readToken("--warning", el),
    muted: readToken("--text-muted", el),
    bg: readToken("--bg", el),
    font: getComputedStyle(el).getPropertyValue("--font-sans").trim() || "sans-serif",
  };
}

/** Colour of a state. */
export function ciColor(state: CiState, colors: CiColors): string {
  const token = ciToken(state);
  return token ? colors[token] : colors.muted;
}

/**
 * Draws a round badge centred on (`x`, `y`): a ring in the state colour with ✓ / ✗ / – inside,
 * or a filled dot for "running". Draws nothing for `"none"`. Returns the width used (0 or
 * `size`), so callers can shift the next item.
 */
export function drawCiBadge(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  state: CiState,
  colors: CiColors,
  size: number = CI_BADGE_SIZE,
): number {
  if (state === "none") return 0;
  const color = ciColor(state, colors);
  const r = size / 2;
  ctx.save();
  ctx.beginPath();
  ctx.arc(x, y, r - 0.75, 0, Math.PI * 2);
  ctx.fillStyle = colors.bg;
  ctx.fill();
  ctx.lineWidth = 1.5;
  ctx.strokeStyle = color;
  ctx.stroke();
  if (state === "pending") {
    ctx.beginPath();
    ctx.arc(x, y, r * 0.42, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.fill();
  } else {
    ctx.fillStyle = color;
    ctx.font = `700 ${Math.round(size * 0.68)}px ${colors.font}`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(ciGlyph(state), x, y + 0.5);
  }
  ctx.restore();
  return size;
}
