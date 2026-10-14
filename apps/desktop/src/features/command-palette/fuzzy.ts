/**
 * Small fuzzy scorer for the command palette. Case- and Vietnamese-diacritics-insensitive
 * ("chuyen nhanh" finds "Chuyển nhánh"). Higher is better; `null` means no match.
 *
 * Tiers for one query token against one text: exact > prefix > word-start prefix > substring >
 * word-start acronym ("cb" → "Create Branch") > plain subsequence (scored by how tight it is).
 */

const COMBINING_MARKS = /[\u0300-\u036f]/g;

/** Lower-case, strip diacritics (NFD + combining marks) and map `đ` → `d`. */
export function normalize(text: string): string {
  return text.normalize("NFD").replace(COMBINING_MARKS, "").replace(/[đĐ]/g, "d").toLowerCase();
}

const isBoundary = (text: string, i: number) => i === 0 || /[\s\-_./:()…,]/.test(text[i - 1]!);

function wordStarts(text: string): number[] {
  const starts: number[] = [];
  for (let i = 0; i < text.length; i++) {
    if (!/\s/.test(text[i]!) && isBoundary(text, i)) starts.push(i);
  }
  return starts;
}

/** Tight subsequence score: rewards consecutive and word-start hits, punishes gaps. */
function subsequence(q: string, text: string): number | null {
  let score = 0;
  let prev = -1;
  let from = 0;
  for (const ch of q) {
    const i = text.indexOf(ch, from);
    if (i < 0) return null;
    if (prev >= 0 && i === prev + 1) score += 6;
    if (isBoundary(text, i)) score += 8;
    if (prev >= 0) score -= Math.min(i - prev - 1, 6);
    prev = i;
    from = i + 1;
  }
  return Math.max(1, 100 + score);
}

/** Every query char at a word start, in order ("cb" in "create branch"). */
function acronym(q: string, text: string): boolean {
  const starts = wordStarts(text);
  let k = 0;
  for (const s of starts) {
    if (k < q.length && text[s] === q[k]) k++;
  }
  return k === q.length;
}

/** Scores one normalized token against one normalized text. */
export function scoreToken(q: string, text: string): number | null {
  if (!q) return 0;
  if (!text) return null;
  // Shorter texts win ties inside a tier.
  const tie = Math.max(0, 40 - text.length) / 100;
  if (text === q) return 1000;
  if (text.startsWith(q)) return 900 + tie;
  const at = text.indexOf(q);
  if (at > 0) {
    let i = at;
    while (i >= 0) {
      if (isBoundary(text, i)) return 700 + tie;
      i = text.indexOf(q, i + 1);
    }
    return 500 + tie;
  }
  if (q.length > 1 && acronym(q, text)) return 400 + tie;
  const sub = subsequence(q, text);
  return sub === null ? null : Math.min(sub, 300) + tie;
}

export interface Scorable {
  title: string;
  keywords?: readonly string[];
}

/** Keyword hits count a little less than title hits. */
const KEYWORD_WEIGHT = 0.8;

/**
 * Scores a query against a title and keywords. Every whitespace-separated token must match
 * the title or a keyword; the whole query matching the title as one piece adds a bonus.
 */
export function scoreCommand(query: string, item: Scorable): number | null {
  const q = normalize(query).trim();
  if (!q) return 0;
  const title = normalize(item.title);
  const keywords = (item.keywords ?? []).map(normalize);
  const tokens = q.split(/\s+/);
  let total = 0;
  for (const token of tokens) {
    let best = scoreToken(token, title);
    for (const kw of keywords) {
      const s = scoreToken(token, kw);
      if (s !== null && (best === null || s * KEYWORD_WEIGHT > best)) best = s * KEYWORD_WEIGHT;
    }
    if (best === null) return null;
    total += best;
  }
  if (tokens.length > 1) {
    const whole = scoreToken(q, title);
    if (whole !== null && whole >= 500) total += whole;
  }
  return total;
}

/** Filters and sorts `items` by score (stable for equal scores). Empty query keeps all. */
export function fuzzyFilter<T extends Scorable>(query: string, items: readonly T[]): T[] {
  if (!query.trim()) return [...items];
  return items
    .map((item, index) => ({ item, index, score: scoreCommand(query, item) }))
    .filter((r): r is { item: T; index: number; score: number } => r.score !== null)
    .sort((a, b) => b.score - a.score || a.index - b.index)
    .map((r) => r.item);
}
