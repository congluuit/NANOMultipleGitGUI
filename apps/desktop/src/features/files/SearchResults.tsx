/** Result lists of the search panel: matching lines grouped by file, with the match marked. */
import { ChevronDown, ChevronRight, FileText } from "lucide-react";
import type { SearchMatch, SearchOptions } from "@/ipc";
import { cn } from "@/lib/utils";
import { matchLength } from "./search";
import { baseName, parentOf } from "./tree";

/** Files (collapsible) and their matching lines; a click on a line calls `onOpen`. */
export function FileGroups({
  groups,
  collapsed,
  onToggle,
  onOpen,
  query,
  options,
  keyPrefix = "",
  nested = false,
}: {
  groups: [string, SearchMatch[]][];
  /** Collapsed entries, as `keyPrefix + path`. */
  collapsed: ReadonlySet<string>;
  onToggle: (key: string) => void;
  onOpen: (match: SearchMatch) => void;
  query: string;
  options: SearchOptions;
  keyPrefix?: string;
  /** Inside a repository group: indented one more level. */
  nested?: boolean;
}) {
  return (
    <>
      {groups.map(([path, matches]) => {
        const key = keyPrefix + path;
        const open = !collapsed.has(key);
        const Chevron = open ? ChevronDown : ChevronRight;
        return (
          <li key={path}>
            <button
              type="button"
              title={path}
              aria-expanded={open}
              onClick={() => onToggle(key)}
              className={cn(
                "flex h-6 w-full items-center gap-1 pr-2 text-left text-xs hover:bg-surface-2",
                nested ? "pl-5" : "pl-2",
              )}
            >
              <Chevron className="size-3.5 shrink-0 text-muted-foreground" />
              <FileText className="size-3.5 shrink-0 text-muted-foreground" />
              <span className="shrink-0 font-medium">{baseName(path)}</span>
              <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground">
                {parentOf(path)}
              </span>
              <span className="shrink-0 rounded bg-surface-2 px-1 text-[10px]">
                {matches.length}
              </span>
            </button>
            {open && (
              <ul>
                {matches.map((m) => (
                  <li key={`${m.line}:${m.column}`}>
                    <button
                      type="button"
                      onClick={() => onOpen(m)}
                      className={cn(
                        "flex w-full items-baseline gap-2 py-0.5 pr-2 text-left text-xs hover:bg-surface-2",
                        nested ? "pl-11" : "pl-8",
                      )}
                    >
                      <span className="w-8 shrink-0 text-right font-mono text-[10px] text-muted-foreground tabular-nums">
                        {m.line}
                      </span>
                      <span className="min-w-0 flex-1 truncate font-mono text-[11px]">
                        <Highlighted match={m} query={query} options={options} />
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </li>
        );
      })}
    </>
  );
}

function Highlighted({
  match,
  query,
  options,
}: {
  match: SearchMatch;
  query: string;
  options: SearchOptions;
}) {
  const start = Math.max(0, match.column - 1 - match.textStart);
  const len = matchLength(match.text, start, query, options);
  const text = match.text;
  // Long lines: start a little before the match so it stays visible.
  const from = Math.max(0, start - 12);
  return (
    <>
      {from > 0 && "..."}
      {text.slice(from, start).trimStart()}
      <mark className="rounded-sm bg-warning/30 text-foreground">
        {text.slice(start, start + len)}
      </mark>
      {text.slice(start + len)}
    </>
  );
}
