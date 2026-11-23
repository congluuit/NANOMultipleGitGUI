/**
 * A small Markdown parser for the editor's preview: CommonMark blocks (headings, paragraphs,
 * fenced and indented code, quotes, nested lists, rules) plus the GitHub additions people use
 * in READMEs (tables, task lists, strikethrough, bare links). It produces a tree that
 * `MarkdownPreview` renders as React elements, so nothing in the file ever reaches the DOM as
 * HTML: raw HTML is kept as literal text and links only open through the app.
 *
 * It aims at reading a file comfortably, not at matching every corner of the specification
 * (reference links and nested emphasis edge cases are rendered as plain text).
 */

export type Inline =
  | { t: "text"; v: string }
  | { t: "code"; v: string }
  | { t: "em" | "strong" | "del"; c: Inline[] }
  | { t: "link"; href: string; c: Inline[] }
  | { t: "image"; src: string; alt: string }
  | { t: "br" };

export type Align = "left" | "center" | "right" | null;

export interface ListItem {
  /** `true` / `false` for a task list item (`- [x]`), `null` otherwise. */
  checked: boolean | null;
  c: Block[];
}

export type Block =
  | { t: "heading"; level: number; id: string; c: Inline[] }
  | { t: "p"; c: Inline[] }
  | { t: "code"; lang: string; v: string }
  | { t: "quote"; c: Block[] }
  /** `tight`: no blank line between the items (less space between them). */
  | { t: "list"; ordered: boolean; start: number; tight: boolean; items: ListItem[] }
  | { t: "hr" }
  | { t: "table"; align: Align[]; head: Inline[][]; rows: Inline[][][] }
  /** Raw HTML, shown as text. */
  | { t: "html"; v: string };

const FENCE = /^ {0,3}(`{3,}|~{3,})\s*([^\s`]*)[^`]*$/;
const HEADING = /^ {0,3}(#{1,6})(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$/;
const RULE = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
const QUOTE = /^ {0,3}> ?(.*)$/;
const LIST_ITEM = /^( {0,3})([-*+]|\d{1,9}[.)])([ \t]+|$)(.*)$/;
const HTML_START = /^ {0,3}<(?:[a-zA-Z][\w-]*[\s/>]|[a-zA-Z][\w-]*$|\/[a-zA-Z]|!--|\?|!\[CDATA\[)/;
const TABLE_DELIMITER = /^ {0,3}\|?[ \t]*:?-+:?[ \t]*(?:\|[ \t]*:?-+:?[ \t]*)*\|?[ \t]*$/;
const SETEXT = /^ {0,3}(=+|-+)[ \t]*$/;

const isBlank = (line: string) => line.trim() === "";
const expandTabs = (line: string) => line.replace(/\t/g, "    ");

/** Whether `line` starts a block that interrupts a paragraph. */
function interrupts(line: string): boolean {
  if (FENCE.test(line) || HEADING.test(line) || RULE.test(line) || QUOTE.test(line)) return true;
  if (HTML_START.test(line)) return true;
  const item = LIST_ITEM.exec(line);
  // An ordered list interrupts a paragraph only when it starts at 1, and never empty.
  return (
    !!item && !!item[4]?.trim() && (/^[-*+]$/.test(item[2] ?? "") || /^1[.)]$/.test(item[2] ?? ""))
  );
}

function splitRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|") && !s.endsWith("\\|")) s = s.slice(0, -1);
  const cells: string[] = [];
  let cell = "";
  let code = false;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (ch === "\\" && s[i + 1] === "|") {
      cell += "|";
      i++;
    } else if (ch === "`") {
      code = !code;
      cell += ch;
    } else if (ch === "|" && !code) {
      cells.push(cell.trim());
      cell = "";
    } else {
      cell += ch;
    }
  }
  cells.push(cell.trim());
  return cells;
}

/** GitHub-style heading anchors: lower case, punctuation dropped, spaces to dashes. */
export function slug(text: string): string {
  return text
    .toLowerCase()
    .trim()
    .replace(/[^\p{L}\p{N}\s_-]/gu, "")
    .replace(/\s/g, "-");
}

function plainText(inlines: Inline[]): string {
  return inlines
    .map((i) => {
      switch (i.t) {
        case "text":
        case "code":
          return i.v;
        case "image":
          return i.alt;
        case "br":
          return " ";
        default:
          return plainText(i.c);
      }
    })
    .join("");
}

class BlockParser {
  private ids = new Map<string, number>();

  parse(lines: string[]): Block[] {
    const out: Block[] = [];
    let i = 0;
    while (i < lines.length) {
      const line = lines[i] ?? "";
      if (isBlank(line)) {
        i++;
        continue;
      }
      const fence = FENCE.exec(line);
      if (fence) {
        const marker = fence[1] ?? "```";
        const indent = line.length - line.trimStart().length;
        const body: string[] = [];
        i++;
        while (i < lines.length) {
          const l = lines[i] ?? "";
          const close = new RegExp(
            `^ {0,3}${marker[0] === "`" ? "`" : "~"}{${marker.length},}\\s*$`,
          );
          if (close.test(l)) {
            i++;
            break;
          }
          body.push(l.replace(new RegExp(`^ {0,${indent}}`), ""));
          i++;
        }
        out.push({ t: "code", lang: fence[2] ?? "", v: body.join("\n") });
        continue;
      }
      const heading = HEADING.exec(line);
      if (heading) {
        out.push(this.heading((heading[1] ?? "#").length, heading[2] ?? ""));
        i++;
        continue;
      }
      if (RULE.test(line)) {
        out.push({ t: "hr" });
        i++;
        continue;
      }
      if (QUOTE.test(line)) {
        const inner: string[] = [];
        while (i < lines.length) {
          const l = lines[i] ?? "";
          const q = QUOTE.exec(l);
          if (q) inner.push(q[1] ?? "");
          // A paragraph line without ">" continues the quote ("lazy" continuation).
          else if (!isBlank(l) && !interrupts(l) && inner.length && !isBlank(inner.at(-1) ?? ""))
            inner.push(l);
          else break;
          i++;
        }
        out.push({ t: "quote", c: this.parse(inner) });
        continue;
      }
      if (LIST_ITEM.test(line)) {
        i = this.list(lines, i, out);
        continue;
      }
      if (HTML_START.test(line)) {
        const html: string[] = [];
        while (i < lines.length && !isBlank(lines[i] ?? "")) html.push(lines[i++] ?? "");
        out.push({ t: "html", v: html.join("\n") });
        continue;
      }
      if (/^( {4}|\t)/.test(line)) {
        const code: string[] = [];
        while (i < lines.length) {
          const l = expandTabs(lines[i] ?? "");
          if (l.startsWith("    ")) code.push(l.slice(4));
          else if (isBlank(l)) code.push("");
          else break;
          i++;
        }
        while (code.length && isBlank(code.at(-1) ?? "")) code.pop();
        out.push({ t: "code", lang: "", v: code.join("\n") });
        continue;
      }
      if (line.includes("|") && TABLE_DELIMITER.test(lines[i + 1] ?? "")) {
        const head = splitRow(line);
        const align = splitRow(lines[i + 1] ?? "").map((d): Align => {
          const left = d.startsWith(":");
          const right = d.endsWith(":");
          return left && right ? "center" : right ? "right" : left ? "left" : null;
        });
        if (align.length === head.length) {
          i += 2;
          const rows: Inline[][][] = [];
          while (i < lines.length && !isBlank(lines[i] ?? "") && !interrupts(lines[i] ?? "")) {
            const cells = splitRow(lines[i] ?? "");
            rows.push(head.map((_, k) => parseInline(cells[k] ?? "")));
            i++;
          }
          out.push({ t: "table", align, head: head.map((h) => parseInline(h)), rows });
          continue;
        }
      }
      // A paragraph, or a setext heading when underlined with === or ---.
      const para: string[] = [line];
      i++;
      let setext = 0;
      while (i < lines.length) {
        const l = lines[i] ?? "";
        const underline = SETEXT.exec(l);
        if (underline) {
          setext = (underline[1] ?? "=").startsWith("=") ? 1 : 2;
          i++;
          break;
        }
        if (isBlank(l) || interrupts(l)) break;
        para.push(l);
        i++;
      }
      const text = para.map((l) => l.trimStart()).join("\n");
      out.push(setext ? this.heading(setext, text) : { t: "p", c: parseInline(text.trimEnd()) });
    }
    return out;
  }

  private heading(level: number, text: string): Block {
    const c = parseInline(text.trim());
    const base = slug(plainText(c));
    const n = this.ids.get(base) ?? 0;
    this.ids.set(base, n + 1);
    return { t: "heading", level, id: n ? `${base}-${n}` : base, c };
  }

  /** Parses the list starting at line `start`; returns the line after it. */
  private list(lines: string[], start: number, out: Block[]): number {
    const first = LIST_ITEM.exec(lines[start] ?? "");
    const marker = first?.[2] ?? "-";
    const ordered = /\d/.test(marker);
    const kind = ordered ? marker.slice(-1) : marker;
    const items: ListItem[] = [];
    let loose = false;
    let i = start;
    /** The line starts another item of this list. */
    const sameList = (line: string | undefined) => {
      const m = LIST_ITEM.exec(line ?? "");
      return !!m && (/\d/.test(m[2] ?? "") ? (m[2] ?? "").slice(-1) : m[2]) === kind;
    };
    while (i < lines.length && sameList(lines[i])) {
      const m = LIST_ITEM.exec(lines[i] ?? "");
      if (!m) break;
      const width =
        (m[1] ?? "").length + (m[2] ?? "").length + Math.min((m[3] ?? " ").length || 1, 4);
      const body: string[] = [m[4] ?? ""];
      i++;
      let blankBefore = false;
      while (i < lines.length) {
        const raw = expandTabs(lines[i] ?? "");
        if (isBlank(raw)) {
          body.push("");
          blankBefore = true;
          i++;
          continue;
        }
        const indent = raw.length - raw.trimStart().length;
        if (indent >= width) {
          body.push(raw.slice(width));
          if (blankBefore) loose = true;
          blankBefore = false;
          i++;
          continue;
        }
        // A lazy continuation of the item's paragraph.
        if (!blankBefore && !interrupts(raw) && !LIST_ITEM.test(raw)) {
          body.push(raw.trimStart());
          i++;
          continue;
        }
        break;
      }
      // Trailing blank lines belong between items, not to this one.
      let trailing = 0;
      while (body.length > 1 && isBlank(body.at(-1) ?? "")) {
        body.pop();
        trailing++;
      }
      if (trailing && sameList(lines[i])) loose = true;
      let checked: boolean | null = null;
      const task = /^\[([ xX])\][ \t]+/.exec(body[0] ?? "");
      if (task) {
        checked = task[1] !== " ";
        body[0] = (body[0] ?? "").slice(task[0].length);
      }
      items.push({ checked, c: this.parse(body) });
      if (trailing && !sameList(lines[i])) break;
    }
    out.push({
      t: "list",
      ordered,
      start: ordered ? Number.parseInt(marker, 10) : 1,
      tight: !loose,
      items,
    });
    return i;
  }
}

export function parseMarkdown(text: string): Block[] {
  return new BlockParser().parse(text.replace(/\r\n?/g, "\n").split("\n"));
}

// ---------------------------------------------------------------------------------------------
// Inline elements.

const PUNCT = /[!"#$%&'()*+,\-./:;<=>?@[\\\]^_`{|}~]/;
const BARE_URL = /^https?:\/\/[^\s<>"]*[^\s<>".,;:!?)'\]*_~]/;

/** The index of the `]` closing the `[` at `open`, or -1. */
function closingBracket(s: string, open: number): number {
  let depth = 0;
  for (let i = open; i < s.length; i++) {
    const ch = s[i];
    if (ch === "\\") i++;
    else if (ch === "`") {
      const end = s.indexOf("`", i + 1);
      if (end > 0) i = end;
    } else if (ch === "[") depth++;
    else if (ch === "]" && --depth === 0) return i;
  }
  return -1;
}

/** `(url "title")` right after a link's text: the URL and where it ends. */
function linkTarget(s: string, at: number): { href: string; end: number } | null {
  if (s[at] !== "(") return null;
  let depth = 0;
  for (let i = at; i < s.length; i++) {
    const ch = s[i];
    if (ch === "\\") i++;
    else if (ch === "(") depth++;
    else if (ch === ")" && --depth === 0) {
      let inner = s.slice(at + 1, i).trim();
      // Drop an optional title.
      inner = inner.replace(/\s+("[^"]*"|'[^']*'|\([^)]*\))$/, "");
      if (inner.startsWith("<") && inner.endsWith(">")) inner = inner.slice(1, -1);
      if (/\s/.test(inner)) return null;
      return { href: inner, end: i + 1 };
    }
  }
  return null;
}

const DELIMS = [
  { mark: "**", t: "strong" },
  { mark: "__", t: "strong" },
  { mark: "~~", t: "del" },
  { mark: "*", t: "em" },
  { mark: "_", t: "em" },
] as const;

/** Where the emphasis opened at `at` with `mark` closes, or -1. */
function closingDelim(s: string, at: number, mark: string): number {
  let i = at + mark.length;
  if (/\s/.test(s[i] ?? " ")) return -1;
  for (; i < s.length; i++) {
    const ch = s[i];
    if (ch === "\\") {
      i++;
      continue;
    }
    if (ch === "`") {
      const end = s.indexOf("`", i + 1);
      if (end > 0) i = end;
      continue;
    }
    if (s.startsWith(mark, i) && i > at + mark.length && !/\s/.test(s[i - 1] ?? " ")) {
      // `*` inside `**`: skip the double one.
      if (mark.length === 1 && s[i + 1] === mark && s[i - 1] !== mark) {
        i++;
        continue;
      }
      // `_` does not close inside a word.
      if (mark[0] === "_" && /[\p{L}\p{N}]/u.test(s[i + mark.length] ?? "")) continue;
      return i;
    }
  }
  return -1;
}

export function parseInline(s: string): Inline[] {
  const out: Inline[] = [];
  let text = "";
  const flush = () => {
    if (text) out.push({ t: "text", v: text });
    text = "";
  };
  for (let i = 0; i < s.length;) {
    const ch = s[i] ?? "";
    const rest = s.slice(i);
    if (ch === "\\" && s[i + 1] === "\n") {
      flush();
      out.push({ t: "br" });
      i += 2;
      continue;
    }
    if (ch === "\\" && PUNCT.test(s[i + 1] ?? "")) {
      text += s[i + 1];
      i += 2;
      continue;
    }
    if (ch === "\n") {
      // Two spaces before a line break: a hard break; otherwise a space.
      if (text.endsWith("  ")) {
        text = text.trimEnd();
        flush();
        out.push({ t: "br" });
      } else {
        text = text.trimEnd() + " ";
      }
      i++;
      continue;
    }
    if (ch === "`") {
      const run = /^`+/.exec(rest)?.[0] ?? "`";
      const end = s.indexOf(run, i + run.length);
      if (end > 0 && s[end + run.length] !== "`") {
        flush();
        let v = s.slice(i + run.length, end).replace(/\n/g, " ");
        if (v.startsWith(" ") && v.endsWith(" ") && v.trim()) v = v.slice(1, -1);
        out.push({ t: "code", v });
        i = end + run.length;
        continue;
      }
      text += run;
      i += run.length;
      continue;
    }
    if (ch === "!" && s[i + 1] === "[") {
      const close = closingBracket(s, i + 1);
      const target = close > 0 ? linkTarget(s, close + 1) : null;
      if (target) {
        flush();
        out.push({ t: "image", alt: s.slice(i + 2, close), src: target.href });
        i = target.end;
        continue;
      }
    }
    if (ch === "[") {
      const close = closingBracket(s, i);
      const target = close > 0 ? linkTarget(s, close + 1) : null;
      if (target) {
        flush();
        out.push({ t: "link", href: target.href, c: parseInline(s.slice(i + 1, close)) });
        i = target.end;
        continue;
      }
    }
    if (ch === "<") {
      const auto = /^<((?:https?|mailto):[^\s<>]+)>/i.exec(rest);
      if (auto) {
        flush();
        out.push({ t: "link", href: auto[1] ?? "", c: [{ t: "text", v: auto[1] ?? "" }] });
        i += auto[0].length;
        continue;
      }
    }
    if ((ch === "h" || ch === "H") && !/[\p{L}\p{N}]/u.test(s[i - 1] ?? "")) {
      const url = BARE_URL.exec(rest);
      if (url) {
        flush();
        out.push({ t: "link", href: url[0], c: [{ t: "text", v: url[0] }] });
        i += url[0].length;
        continue;
      }
    }
    const delim = DELIMS.find((d) => rest.startsWith(d.mark));
    if (delim && !(delim.mark[0] === "_" && /[\p{L}\p{N}]/u.test(s[i - 1] ?? ""))) {
      const end = closingDelim(s, i, delim.mark);
      if (end > 0) {
        flush();
        out.push({ t: delim.t, c: parseInline(s.slice(i + delim.mark.length, end)) });
        i = end + delim.mark.length;
        continue;
      }
      text += delim.mark;
      i += delim.mark.length;
      continue;
    }
    text += ch;
    i++;
  }
  flush();
  return out;
}
