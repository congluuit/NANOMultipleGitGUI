import { useEffect, useLayoutEffect, useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Splitter } from "@/components/layout/Splitter";
import { cn } from "@/lib/utils";
import {
  maxWidthFor,
  PANEL_LIMITS,
  RAIL_WIDTH,
  useLayout,
  type PanelId,
  useFittedPanels,
} from "@/stores/layout";

/** The Sidebar | middle | right panel row; reports its width so the panels can fit. */
export function PanelRow({ hasRight, children }: { hasRight: boolean; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const report = () => useLayout.getState().setRow(Math.round(el.clientWidth), hasRight);
    report();
    const ro = new ResizeObserver(report);
    ro.observe(el);
    return () => ro.disconnect();
  }, [hasRight]);
  return (
    <div ref={ref} className="relative flex min-h-0 flex-1" data-testid="panel-row">
      {children}
    </div>
  );
}

const PANEL_DOM_ID: Record<PanelId, string> = { sidebar: "shell-sidebar", right: "shell-right" };

/**
 * A resizable side panel. Collapsed (by the user or for lack of room) it shows `rail`; the full
 * content stays mounted but hidden so drafts and scroll positions survive, and is shown over
 * the middle while "peeking" (expanded temporarily in a window too narrow for it).
 */
export function SidePanel({
  id,
  rail,
  children,
}: {
  id: PanelId;
  rail: ReactNode;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const fit = useFittedPanels();
  const saved = useLayout((s) => s[id]);
  const rowWidth = useLayout((s) => s.rowWidth);
  const peeking = useLayout((s) => s.peek === id);
  const overlay = useRef<HTMLDivElement>(null);
  const isRail = id === "sidebar" ? fit.sidebarRail : fit.rightRail;
  const width = id === "sidebar" ? fit.sidebar : fit.right;
  const other = id === "sidebar" ? fit.right : fit.sidebar;
  const showPeek = isRail && peeking;

  // A peek closes on Escape or a click outside it (menus / dialogs it opened do not count).
  useEffect(() => {
    if (!showPeek) return;
    const close = () => useLayout.getState().setPeek(null);
    const onDown = (e: PointerEvent) => {
      const target = e.target as Element | null;
      if (!target || overlay.current?.contains(target)) return;
      if (target.closest('[data-panel-rail], [data-radix-popper-content-wrapper], [role="dialog"]'))
        return;
      close();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && !e.defaultPrevented && close();
    document.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [showPeek]);

  const splitter = !isRail && (
    <Splitter
      side={id === "sidebar" ? "left" : "right"}
      label={t(id === "sidebar" ? "layout.resizeSidebar" : "layout.resizeRight")}
      controls={PANEL_DOM_ID[id]}
      value={width}
      min={PANEL_LIMITS[id].min}
      max={rowWidth > 0 ? maxWidthFor(id, rowWidth, other) : PANEL_LIMITS[id].max}
      defaultValue={PANEL_LIMITS[id].default}
      onChange={(px, final) => useLayout.getState().setSize(id, px, final)}
    />
  );

  return (
    <>
      {id === "right" && splitter}
      {isRail && (
        <div
          data-panel-rail={id}
          style={{ width: RAIL_WIDTH }}
          className={cn(
            "flex shrink-0 flex-col items-center gap-1 bg-surface py-2",
            id === "sidebar" ? "border-r" : "border-l",
          )}
        >
          {rail}
        </div>
      )}
      <div
        ref={overlay}
        id={PANEL_DOM_ID[id]}
        data-testid={`panel-${id}`}
        style={{
          width: isRail ? saved : width,
          ...(showPeek && { [id === "sidebar" ? "left" : "right"]: RAIL_WIDTH }),
        }}
        className={cn(
          "flex min-w-0",
          !isRail && "shrink-0",
          isRail && !showPeek && "hidden",
          showPeek && "absolute inset-y-0 z-30 border-x shadow-xl",
        )}
      >
        {children}
      </div>
      {id === "sidebar" && splitter}
    </>
  );
}
