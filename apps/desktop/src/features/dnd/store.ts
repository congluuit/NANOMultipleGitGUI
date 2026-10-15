import { create } from "zustand";

/** A branch being dragged (pointer events, not HTML5 drag-and-drop: works in every webview). */
export interface BranchDrag {
  repoId: number;
  source: string;
  x: number;
  y: number;
  /** Branch under the pointer, if any. */
  over: string | null;
}

/** A branch dropped on another one: the drop menu is open at `x`, `y`. */
export interface BranchDrop {
  repoId: number;
  source: string;
  target: string;
  x: number;
  y: number;
}

interface DndStore {
  drag: BranchDrag | null;
  drop: BranchDrop | null;
  setDrag: (drag: BranchDrag | null) => void;
  setDrop: (drop: BranchDrop | null) => void;
}

export const useDnd = create<DndStore>()((set) => ({
  drag: null,
  drop: null,
  setDrag: (drag) => set({ drag }),
  setDrop: (drop) => set({ drop }),
}));

/**
 * Drop targets that cannot carry a `data-drop-branch` attribute (the canvas graph): the element
 * carries `data-drop-resolver="<id>"` and registers a function turning a point into a branch.
 */
const resolvers = new Map<string, (x: number, y: number) => string | null>();

export function registerDropResolver(id: string, fn: (x: number, y: number) => string | null) {
  resolvers.set(id, fn);
  return () => {
    if (resolvers.get(id) === fn) resolvers.delete(id);
  };
}

/** The local branch under a point: `data-drop-branch` elements first, then resolvers. */
export function branchAt(x: number, y: number): string | null {
  const elements =
    typeof document.elementsFromPoint === "function" ? document.elementsFromPoint(x, y) : [];
  for (const el of elements) {
    const branch = el.closest<HTMLElement>("[data-drop-branch]")?.dataset.dropBranch;
    if (branch) return branch;
    const id = el.closest<HTMLElement>("[data-drop-resolver]")?.dataset.dropResolver;
    if (id) {
      const found = resolvers.get(id)?.(x, y) ?? null;
      if (found) return found;
    }
  }
  return null;
}

/** Pixels the pointer must travel before a press becomes a drag. */
export const DRAG_THRESHOLD = 5;

/**
 * Starts tracking a possible drag of `source` from a pointer press. A click (no movement) is
 * left alone; after a real drag the click that follows is swallowed and, when released over
 * another local branch, the drop menu opens.
 */
export function startBranchDrag(
  e: { clientX: number; clientY: number; button: number },
  repoId: number,
  source: string,
) {
  if (e.button !== 0) return;
  const startX = e.clientX;
  const startY = e.clientY;
  let active = false;
  const { setDrag, setDrop } = useDnd.getState();

  const onMove = (ev: PointerEvent) => {
    if (!active) {
      if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < DRAG_THRESHOLD) return;
      active = true;
      document.body.style.userSelect = "none";
    }
    const over = branchAt(ev.clientX, ev.clientY);
    setDrag({
      repoId,
      source,
      x: ev.clientX,
      y: ev.clientY,
      over: over === source ? null : over,
    });
  };
  const finish = (ev: { clientX: number; clientY: number }, cancelled: boolean) => {
    window.removeEventListener("pointermove", onMove, true);
    window.removeEventListener("pointerup", onUp, true);
    window.removeEventListener("pointercancel", onCancel, true);
    window.removeEventListener("keydown", onKey, true);
    if (!active) return;
    document.body.style.userSelect = "";
    setDrag(null);
    // The click that ends a drag must not also activate the row under the pointer.
    const swallow = (c: Event) => {
      c.stopPropagation();
      c.preventDefault();
    };
    window.addEventListener("click", swallow, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", swallow, true), 0);
    if (cancelled) return;
    const target = branchAt(ev.clientX, ev.clientY);
    if (target && target !== source) {
      setDrop({ repoId, source, target, x: ev.clientX, y: ev.clientY });
    }
  };
  const onUp = (ev: PointerEvent) => finish(ev, false);
  const onCancel = (ev: PointerEvent) => finish(ev, true);
  const onKey = (ev: KeyboardEvent) => {
    if (ev.key === "Escape") finish({ clientX: 0, clientY: 0 }, true);
  };
  window.addEventListener("pointermove", onMove, true);
  window.addEventListener("pointerup", onUp, true);
  window.addEventListener("pointercancel", onCancel, true);
  window.addEventListener("keydown", onKey, true);
}
