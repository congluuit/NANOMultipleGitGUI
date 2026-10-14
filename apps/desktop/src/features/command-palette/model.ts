import { scoreCommand } from "./fuzzy";
import { isEnabled, type PaletteCommand, type PaletteContext } from "./registry";
import { RECENT_LIMIT } from "./store";

/** One row of the palette. */
export interface PaletteItem {
  /** `cmd.id`, or `parent/child` for a child offered at the top level. */
  key: string;
  cmd: PaletteCommand;
  title: string;
  enabled: boolean;
  /** Parent of a flattened child (running it runs the child). */
  parent: PaletteCommand | null;
}

export interface PaletteSection {
  /** Heading; empty inside a sub-list. */
  group: string;
  items: PaletteItem[];
}

const idWords = (id: string) => id.split(/[./_-]+/).filter(Boolean);

/** "Theme…" → "Theme" (for "Theme: Dark"). */
const stem = (title: string) => title.replace(/(…|\.\.\.)$/, "").trim();

const runnable = (cmd: PaletteCommand) => !!(cmd.run || cmd.children || cmd.input);

function item(
  cmd: PaletteCommand,
  ctx: PaletteContext,
  parent: PaletteCommand | null = null,
  parentEnabled = true,
): PaletteItem {
  return {
    key: parent ? `${parent.id}/${cmd.id}` : cmd.id,
    cmd,
    title: parent ? `${stem(parent.title)}: ${cmd.title}` : cmd.title,
    enabled: parentEnabled && runnable(cmd) && isEnabled(cmd, ctx),
    parent,
  };
}

const keywordsOf = (it: PaletteItem) => [
  ...(it.parent?.keywords ?? []),
  ...(it.parent ? idWords(it.parent.id) : []),
  ...(it.cmd.keywords ?? []),
  ...idWords(it.cmd.id),
];

const groupOf = (it: PaletteItem) => (it.parent ?? it.cmd).group;

function grouped(items: PaletteItem[]): PaletteSection[] {
  const sections = new Map<string, PaletteItem[]>();
  for (const it of items) {
    const g = groupOf(it);
    const list = sections.get(g);
    if (list) list.push(it);
    else sections.set(g, [it]);
  }
  return [...sections].map(([group, list]) => ({ group, items: list }));
}

/** Children of `flatten` commands, offered as "Parent: Child". */
function flattened(commands: PaletteCommand[], ctx: PaletteContext): PaletteItem[] {
  return commands.flatMap((p) => {
    if (!p.flatten || !p.children) return [];
    const enabled = isEnabled(p, ctx);
    return p.children(ctx).map((c) => item(c, ctx, p, enabled));
  });
}

/**
 * Top-level list. Without a query: "Recent" first, then every command under its group in
 * registration order. With a query: fuzzy matches (recent ones slightly boosted), groups ordered
 * by their best match.
 */
export function topLevelSections(
  commands: PaletteCommand[],
  ctx: PaletteContext,
  query: string,
  recent: readonly string[],
  recentLabel: string,
): PaletteSection[] {
  const base = commands.map((c) => item(c, ctx));
  const children = flattened(commands, ctx);
  const rank = new Map(recent.map((id, i) => [id, i]));

  if (!query.trim()) {
    const byKey = new Map([...base, ...children].map((it) => [it.key, it]));
    const recentItems = recent
      .map((id) => byKey.get(id))
      .filter((it): it is PaletteItem => !!it && it.enabled)
      .slice(0, RECENT_LIMIT);
    const shown = new Set(recentItems.map((it) => it.key));
    const rest = grouped(base.filter((it) => !shown.has(it.key)));
    return recentItems.length ? [{ group: recentLabel, items: recentItems }, ...rest] : rest;
  }

  const scored = [...base, ...children]
    .map((it, index) => {
      const s = scoreCommand(query, { title: it.title, keywords: keywordsOf(it) });
      const r = rank.get(it.key);
      const boost = r === undefined ? 0 : 30 - r;
      // Disabled items sink below enabled ones of a similar score.
      return { it, index, score: s === null ? null : s + boost - (it.enabled ? 0 : 50) };
    })
    .filter((x): x is { it: PaletteItem; index: number; score: number } => x.score !== null)
    .sort((a, b) => b.score - a.score || a.index - b.index);
  return grouped(scored.map((x) => x.it));
}

/** Items of a sub-list step, filtered by `query`. */
export function subSections(
  parent: PaletteCommand,
  ctx: PaletteContext,
  query: string,
): PaletteSection[] {
  const items = (parent.children?.(ctx) ?? []).map((c) => item(c, ctx));
  if (!query.trim()) return items.length ? [{ group: "", items }] : [];
  const scored = items
    .map((it, index) => ({
      it,
      index,
      score: scoreCommand(query, { title: it.title, keywords: keywordsOf(it) }),
    }))
    .filter((x): x is { it: PaletteItem; index: number; score: number } => x.score !== null)
    .sort((a, b) => b.score - a.score || a.index - b.index)
    .map((x) => x.it);
  return scored.length ? [{ group: "", items: scored }] : [];
}

/** Recent-list id to remember after running `cmd` from `parent`'s sub-list (or the top level). */
export function recentId(cmd: PaletteCommand, parent: PaletteCommand | null): string {
  if (!parent) return cmd.id;
  return parent.flatten ? `${parent.id}/${cmd.id}` : parent.id;
}
