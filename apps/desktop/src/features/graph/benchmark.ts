import type { GraphBenchmark } from "@/ipc";
import { ROW_HEIGHT } from "./draw";

export interface FrameStats {
  drawMs: number[];
}

const nextFrame = () => new Promise<number>((r) => requestAnimationFrame(r));

export function percentile(values: number[], p: number): number {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length))]!;
}

/**
 * Scrolls `scroller` at a constant speed (default 600 rows/s ≈ fast trackpad flinging) and
 * measures frame intervals plus canvas draw time, the way a user would perceive smoothness.
 */
export async function runScrollBenchmark(
  scroller: HTMLElement,
  stats: FrameStats,
  opts: { totalRows: number; firstPaintMs: number; durationMs?: number; rowsPerSecond?: number },
): Promise<GraphBenchmark> {
  const duration = opts.durationMs ?? 5000;
  const speed = (opts.rowsPerSecond ?? 600) * ROW_HEIGHT;
  scroller.scrollTop = 0;
  await nextFrame();
  await nextFrame();
  stats.drawMs.length = 0;

  const frames: number[] = [];
  const start = await nextFrame();
  let last = start;
  for (;;) {
    const now = await nextFrame();
    frames.push(now - last);
    last = now;
    const elapsed = now - start;
    scroller.scrollTop = (elapsed / 1000) * speed;
    if (elapsed >= duration) break;
  }

  const draws = stats.drawMs;
  return {
    totalRows: opts.totalRows,
    firstPaintMs: Math.round(opts.firstPaintMs),
    frames: frames.length,
    avgFps: Math.round((frames.length / (duration / 1000)) * 10) / 10,
    p95FrameMs: Math.round(percentile(frames, 95) * 10) / 10,
    maxFrameMs: Math.round(Math.max(...frames) * 10) / 10,
    slowFrames: frames.filter((f) => f > 20).length,
    avgDrawMs:
      Math.round((draws.reduce((a, b) => a + b, 0) / Math.max(1, draws.length)) * 100) / 100,
    maxDrawMs: Math.round(Math.max(0, ...draws) * 100) / 100,
  };
}
