import { create } from "zustand";
import type { ColumnWidths } from "./draw";

/** Same reasons as the panel widths (`stores/layout.ts`): read before the first paint. */
const KEY = "nanomgg.graph.columns";

export type GraphColumn = "refs" | "graph" | "author" | "date";

type Widths = Required<{ [K in GraphColumn]: number | null }>;

function load(): Widths {
  const out: Widths = { refs: null, graph: null, author: null, date: null };
  try {
    const raw: unknown = JSON.parse(globalThis.localStorage?.getItem(KEY) ?? "null");
    if (raw && typeof raw === "object") {
      for (const k of ["refs", "graph", "author", "date"] as const) {
        const v = (raw as Record<string, unknown>)[k];
        if (typeof v === "number" && Number.isFinite(v) && v > 0) out[k] = Math.round(v);
      }
    }
  } catch {
    // Broken value: defaults.
  }
  return out;
}

interface GraphColumnsState {
  widths: ColumnWidths & Widths;
  /** `final` persists the width (false while dragging); `null` = back to the default. */
  setWidth: (column: GraphColumn, px: number | null, final: boolean) => void;
}

export const useGraphColumns = create<GraphColumnsState>((set, get) => ({
  widths: load(),
  setWidth: (column, px, final) => {
    const widths = { ...get().widths, [column]: px };
    set({ widths });
    if (final) {
      try {
        globalThis.localStorage?.setItem(KEY, JSON.stringify(widths));
      } catch {
        // Storage full or disabled: the width still applies for this session.
      }
    }
  },
}));
