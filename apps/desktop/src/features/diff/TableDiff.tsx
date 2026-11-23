import { useMemo, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { errorMessage, type FileContents } from "@/ipc";
import { cn } from "@/lib/utils";
import {
  ROW_PAGE,
  cellValues,
  diffTables,
  displayItems,
  parseSides,
  type KeyChoice,
  type TableColumn,
  type TableDiff as TableDiffData,
  type TableRow,
} from "./csvTable";

interface TableDiffProps {
  contents: { data?: FileContents; isError: boolean; error: unknown };
  path: string;
  /** Switches the diff view to its text modes. */
  onTextView: () => void;
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-xs text-muted-foreground">
      {children}
    </div>
  );
}

/** CSV / TSV diff as a table: rows paired by a key column or by position, changed cells marked. */
export function TableDiff({ contents, path, onTextView }: TableDiffProps) {
  const { t } = useTranslation();
  const c = contents.data;
  const parsed = useMemo(
    () => (c && !c.binary && !c.tooLarge ? parseSides(path, c.original, c.modified) : null),
    [c, path],
  );
  if (contents.isError) {
    return <Centered>{t("diff.error", { message: errorMessage(contents.error) })}</Centered>;
  }
  if (!c) {
    return (
      <Centered>
        <span className="flex items-center gap-2">
          <LoaderCircle className="size-4 animate-spin" />
          {t("diff.loading")}
        </span>
      </Centered>
    );
  }
  if (c.binary) return <Centered>{t("diff.binary")}</Centered>;
  if (c.tooLarge || (parsed && !parsed.ok)) {
    return (
      <Centered>
        <p className="max-w-80">{t("tableDiff.tooLarge")}</p>
        <Button size="xs" variant="outline" onClick={onTextView}>
          {t("tableDiff.showText")}
        </Button>
      </Centered>
    );
  }
  if (!parsed?.ok) return null;
  // Another file starts with fresh header, key and paging choices; a refresh of the same keeps them.
  return <TableView key={path} sides={parsed.sides} />;
}

function TableView({ sides }: { sides: Parameters<typeof diffTables>[0] }) {
  const { t } = useTranslation();
  const [header, setHeader] = useState<boolean | null>(null);
  const [key, setKey] = useState<KeyChoice>("auto");
  const [changesOnly, setChangesOnly] = useState(true);
  const [limit, setLimit] = useState(ROW_PAGE);
  const diff = useMemo(() => diffTables(sides, { header, key }), [sides, header, key]);
  const items = useMemo(
    () => displayItems(diff.rows, changesOnly && !diff.oneSided),
    [diff, changesOnly],
  );
  const shown = items.slice(0, limit);
  const s = diff.stats;
  const nothingChanged =
    !diff.oneSided &&
    s.changed + s.added + s.removed + s.addedColumns + s.removedColumns + s.renamedColumns === 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col" data-testid="table-diff">
      <div className="flex min-h-8 shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b px-3 py-1 text-[11px] text-muted-foreground">
        <label className="flex items-center gap-1">
          <input
            type="checkbox"
            checked={diff.header}
            onChange={(e) => {
              setHeader(e.target.checked);
              setKey("auto");
            }}
            className="accent-primary"
          />
          {t("tableDiff.header")}
        </label>
        {!diff.oneSided && (
          <label className="flex items-center gap-1">
            {t("tableDiff.key")}
            <select
              value={diff.key === null ? "position" : String(diff.key)}
              onChange={(e) =>
                setKey(e.target.value === "position" ? "position" : Number(e.target.value))
              }
              className="max-w-48 rounded border bg-background px-1 py-0.5 text-[11px] text-foreground"
              aria-label={t("tableDiff.key")}
            >
              <option value="position">{t("tableDiff.keyPosition")}</option>
              {diff.keyOptions.map((ci) => (
                <option key={ci} value={ci}>
                  {columnLabel(diff.columns[ci], t("tableDiff.column", { n: ci + 1 }))}
                </option>
              ))}
            </select>
          </label>
        )}
        {!diff.oneSided && (
          <label className="flex items-center gap-1">
            <input
              type="checkbox"
              checked={changesOnly}
              onChange={(e) => setChangesOnly(e.target.checked)}
              className="accent-primary"
            />
            {t("tableDiff.changesOnly")}
          </label>
        )}
        <span className="ml-auto tabular-nums" data-testid="table-stats">
          <Summary diff={diff} />
        </span>
      </div>
      {diff.noise && (
        <div className="shrink-0 border-b bg-surface px-3 py-1.5 text-[11px]" role="status">
          {t("tableDiff.formatOnly", {
            what: diff.noise.map((r) => t(`tableDiff.noise.${r}`)).join(", "),
          })}
        </div>
      )}
      {diff.key !== null && !diff.keyUnique && (
        <div className="shrink-0 border-b bg-warning/10 px-3 py-1.5 text-[11px] text-warning">
          {t("tableDiff.keyNotUnique")}
        </div>
      )}
      {nothingChanged && !diff.noise ? (
        <Centered>{t("tableDiff.noChanges")}</Centered>
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          <table className="border-separate border-spacing-0 font-mono text-[11px]">
            <thead className="sticky top-0 z-10 bg-surface">
              <tr>
                <th className="sticky left-0 z-20 border-r border-b bg-surface px-2 py-1 text-right font-normal text-muted-foreground">
                  {t("tableDiff.oldRow")}
                </th>
                <th className="border-r border-b bg-surface px-2 py-1 text-right font-normal text-muted-foreground">
                  {t("tableDiff.newRow")}
                </th>
                {diff.columns.map((col, ci) => (
                  <ColumnHead key={ci} col={col} index={ci} isKey={diff.key === ci} />
                ))}
              </tr>
            </thead>
            <tbody>
              {shown.map((item, n) =>
                "hidden" in item ? (
                  <tr key={`h${n}`}>
                    <td
                      colSpan={diff.columns.length + 2}
                      className="border-b bg-surface-2/50 px-2 py-0.5 text-muted-foreground"
                    >
                      {t("tableDiff.hiddenRows", { count: item.hidden })}
                    </td>
                  </tr>
                ) : (
                  <Row key={`r${n}`} row={item.row} columns={diff.columns} />
                ),
              )}
            </tbody>
          </table>
          {items.length > shown.length && (
            <div className="flex items-center gap-2 px-3 py-2 text-[11px] text-muted-foreground">
              {t("tableDiff.shownOf", { shown: shown.length, total: items.length })}
              <Button size="xs" variant="outline" onClick={() => setLimit((l) => l + ROW_PAGE)}>
                {t("tableDiff.showMore", {
                  count: Math.min(ROW_PAGE, items.length - shown.length),
                })}
              </Button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** The header text of a column, or `fallback` ("Column 3") when it has none. */
function columnLabel(col: TableColumn | undefined, fallback: string): string {
  return col?.name?.trim() ? col.name : fallback;
}

function Summary({ diff }: { diff: TableDiffData }) {
  const { t } = useTranslation();
  const s = diff.stats;
  if (diff.oneSided) {
    return t("tableDiff.rows", { count: s.added + s.removed });
  }
  const parts: string[] = [];
  if (s.added) parts.push(t("tableDiff.addedRows", { count: s.added }));
  if (s.removed) parts.push(t("tableDiff.removedRows", { count: s.removed }));
  if (s.changed) {
    parts.push(t("tableDiff.changedRows", { count: s.changed, cells: s.changedCells }));
  }
  if (s.addedColumns) parts.push(t("tableDiff.addedColumns", { count: s.addedColumns }));
  if (s.removedColumns) parts.push(t("tableDiff.removedColumns", { count: s.removedColumns }));
  if (s.renamedColumns) parts.push(t("tableDiff.renamedColumns", { count: s.renamedColumns }));
  return parts.join(", ");
}

const COLUMN_TONE: Record<TableColumn["status"], string> = {
  same: "",
  renamed: "bg-warning/15",
  added: "bg-success/15",
  removed: "bg-danger/15",
};

function ColumnHead({ col, index, isKey }: { col: TableColumn; index: number; isKey: boolean }) {
  const { t } = useTranslation();
  const title =
    col.status === "same"
      ? undefined
      : col.status === "renamed"
        ? t("tableDiff.columnRenamed", { name: col.oldName })
        : t(`tableDiff.columnStatus.${col.status}`);
  return (
    <th
      className={cn(
        "max-w-80 border-r border-b bg-surface px-2 py-1 text-left align-bottom font-semibold whitespace-pre-wrap",
        COLUMN_TONE[col.status],
      )}
      title={title}
      data-status={col.status}
    >
      {col.status === "renamed" && (
        <span className="block font-normal text-danger line-through">{col.oldName}</span>
      )}
      <span className={cn(col.status === "removed" && "line-through")}>
        {columnLabel(col, t("tableDiff.column", { n: index + 1 }))}
      </span>
      {isKey && (
        <span className="ml-1 rounded bg-primary/15 px-1 text-[10px] font-normal text-primary">
          {t("tableDiff.keyBadge")}
        </span>
      )}
    </th>
  );
}

const ROW_TONE: Record<TableRow["kind"], string> = {
  same: "",
  changed: "",
  added: "bg-success/10",
  removed: "bg-danger/10",
};

function Row({ row, columns }: { row: TableRow; columns: TableColumn[] }) {
  const { t } = useTranslation();
  const changed = new Set(row.changed);
  const empty = <span className="text-muted-foreground italic">{t("tableDiff.empty")}</span>;
  const show = (v: string) => (v === "" ? empty : v);
  return (
    <tr className={ROW_TONE[row.kind]} data-kind={row.kind}>
      <td
        className={cn(
          "sticky left-0 border-r border-b bg-background px-2 py-0.5 text-right text-muted-foreground tabular-nums",
          row.kind === "removed" && "text-danger",
          row.kind === "changed" && "border-l-2 border-l-warning",
          row.kind === "added" && "border-l-2 border-l-success",
          row.kind === "removed" && "border-l-2 border-l-danger",
        )}
      >
        {row.oldNo ?? ""}
      </td>
      <td
        className={cn(
          "border-r border-b px-2 py-0.5 text-right text-muted-foreground tabular-nums",
          row.kind === "added" && "text-success",
        )}
      >
        {row.newNo ?? ""}
      </td>
      {columns.map((col, ci) => {
        const v = cellValues(row, col);
        const base =
          "max-w-80 border-r border-b px-2 py-0.5 align-top break-words whitespace-pre-wrap";
        if (changed.has(ci)) {
          return (
            <td
              key={ci}
              className={cn(base, "bg-warning/15")}
              title={t("tableDiff.was", { value: v.old })}
              data-changed=""
            >
              <span className="block text-danger line-through">{show(v.old ?? "")}</span>
              <span className="block text-success">{show(v.new ?? "")}</span>
            </td>
          );
        }
        const value = v.new ?? v.old;
        const tone =
          row.kind === "same" || row.kind === "changed"
            ? col.status === "added"
              ? "bg-success/5"
              : col.status === "removed"
                ? "bg-danger/5 text-muted-foreground"
                : ""
            : "";
        return (
          <td key={ci} className={cn(base, tone)}>
            {value === null ? "" : value}
          </td>
        );
      })}
    </tr>
  );
}
