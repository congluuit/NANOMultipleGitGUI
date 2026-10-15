import { useTranslation } from "react-i18next";
import { Splitter } from "@/components/layout/Splitter";
import { useGraphColumns, type GraphColumn } from "./columns";
import { COLUMN_LIMITS, columnLayout, defaultColumnWidths } from "./draw";

/** i18n key of each column's name (resize handle labels). */
const COLUMN_NAME = {
  refs: "graph.columnName.refs",
  graph: "graph.columnName.lanes",
  author: "graph.columnName.author",
  date: "graph.columnName.date",
} as const satisfies Record<GraphColumn, string>;

/**
 * Column titles above the canvas, laid out by the same [`columnLayout`] as the rows, with drag
 * handles between the columns (double-click a handle to restore the default width).
 */
export function GraphHeader({ width, maxLanes }: { width: number; maxLanes: number }) {
  const { t } = useTranslation();
  const widths = useGraphColumns((s) => s.widths);
  const setWidth = useGraphColumns((s) => s.setWidth);
  const cols = columnLayout(width, maxLanes, widths);
  const defaults = defaultColumnWidths(maxLanes);
  // The message column keeps at least this much while another column grows.
  const room = Math.max(0, cols.messageEnd - cols.textX - 180);

  const handle = (column: GraphColumn, x: number, value: number, side: "left" | "right") => (
    <div className="absolute inset-y-0 flex" style={{ left: x }}>
      <Splitter
        side={side}
        label={t("graph.resizeColumn", { column: t(COLUMN_NAME[column]) })}
        value={value}
        min={COLUMN_LIMITS[column].min}
        max={Math.max(COLUMN_LIMITS[column].min, Math.min(COLUMN_LIMITS[column].max, value + room))}
        defaultValue={defaults[column]}
        onChange={(px, final) => setWidth(column, px === defaults[column] ? null : px, final)}
        className="h-full bg-transparent"
      />
    </div>
  );

  const title = "absolute inset-y-0 flex items-center truncate";
  return (
    <div className="relative h-7 shrink-0 overflow-hidden border-b text-[11px] font-semibold whitespace-nowrap text-muted-foreground uppercase">
      <span className={`${title} px-2`} style={{ left: 0, width: cols.refs }}>
        {t("graph.colRefs")}
      </span>
      {handle("refs", cols.refs, cols.refs, "left")}
      <span
        className={`${title} justify-center px-1`}
        style={{ left: cols.graphX, width: cols.graph }}
      >
        {t("graph.colLanes")}
      </span>
      {handle("graph", cols.graphX + cols.graph, cols.graph, "left")}
      <span
        className={title}
        style={{ left: cols.textX, width: Math.max(0, cols.authorX - cols.textX) }}
      >
        {t("graph.colMessage")}
      </span>
      {cols.author > 0 && (
        <>
          {handle("author", cols.authorX - 6, cols.author, "right")}
          <span className={title} style={{ left: cols.authorX, width: cols.author }}>
            {t("graph.colAuthor")}
          </span>
        </>
      )}
      {cols.date > 0 && (
        <>
          {handle("date", cols.dateX - 6, cols.date, "right")}
          <span className={title} style={{ left: cols.dateX, width: cols.date }}>
            {t("graph.colDate")}
          </span>
        </>
      )}
    </div>
  );
}
