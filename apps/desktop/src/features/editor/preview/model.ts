/**
 * Which files the editor can show rendered (Markdown, SVG, JSON), and the pure parts of those
 * previews: JSON formatting with a readable error, the SVG image URL, where a Markdown link
 * leads.
 */

export type PreviewKind = "markdown" | "svg" | "json";

const KINDS: Record<string, PreviewKind> = {
  md: "markdown",
  markdown: "markdown",
  mdown: "markdown",
  mkd: "markdown",
  mdx: "markdown",
  svg: "svg",
  json: "json",
  // Unity assembly definitions and other JSON files under their own extension.
  asmdef: "json",
  asmref: "json",
  webmanifest: "json",
  geojson: "json",
};

export function previewKind(path: string): PreviewKind | null {
  const name = path.split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return null;
  return KINDS[name.slice(dot + 1).toLowerCase()] ?? null;
}

export type JsonResult =
  | { ok: true; pretty: string }
  | { ok: false; message: string; line: number | null; column: number | null };

/** 1-based line and column of a string offset. */
function lineColumn(text: string, offset: number): { line: number; column: number } {
  const before = text.slice(0, offset);
  const lineStart = before.lastIndexOf("\n") + 1;
  return { line: before.split("\n").length, column: offset - lineStart + 1 };
}

/** The text pretty-printed with two spaces, or why it is not JSON (with where, if known). */
export function formatJson(text: string): JsonResult {
  // A BOM is not JSON but editors leave it in; it is no reason to refuse the file.
  const source = text.startsWith("\u{feff}") ? text.slice(1) : text;
  try {
    return { ok: true, pretty: JSON.stringify(JSON.parse(source), null, 2) };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    const lc = /\(line (\d+) column (\d+)\)/.exec(message);
    if (lc) return { ok: false, message, line: Number(lc[1]), column: Number(lc[2]) };
    const at = /position (\d+)/.exec(message);
    if (at) return { ok: false, message, ...lineColumn(source, Number(at[1])) };
    if (/end of (JSON|data) input/i.test(message) || !source.trim()) {
      return { ok: false, message, ...lineColumn(source, source.length) };
    }
    return { ok: false, message, line: null, column: null };
  }
}

/**
 * The SVG as an image URL. An `<img>` never runs the scripts of an SVG nor loads what it links
 * to, and the markup itself never enters the page.
 */
export function svgImageUrl(text: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(text)}`;
}

export type LinkTarget =
  | { kind: "external"; url: string }
  | { kind: "anchor"; id: string }
  | { kind: "file"; path: string; anchor: string | null }
  | { kind: "none" };

/**
 * Where a link of a Markdown file at `from` (repository path) leads: a web page (https only,
 * opened in the browser), a heading of the same file, or another file of the repository.
 */
export function linkTarget(href: string, from: string): LinkTarget {
  const h = href.trim();
  if (/^https:\/\//i.test(h)) return { kind: "external", url: h };
  if (h.startsWith("#"))
    return h.length > 1 ? { kind: "anchor", id: decode(h.slice(1)) } : { kind: "none" };
  // Any other scheme (http:, mailto:, javascript:, file:...) is not followed.
  if (/^[a-z][a-z0-9+.-]*:/i.test(h) || h.startsWith("//")) return { kind: "none" };
  const [pathPart = "", anchor] = h.split("#", 2);
  const clean = decode(pathPart.split("?")[0] ?? "");
  if (!clean) return { kind: "none" };
  const parts = clean.startsWith("/") ? [] : from.split("/").slice(0, -1);
  for (const seg of clean.split("/")) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") {
      if (!parts.length) return { kind: "none" };
      parts.pop();
    } else parts.push(seg);
  }
  if (!parts.length) return { kind: "none" };
  return { kind: "file", path: parts.join("/"), anchor: anchor ? decode(anchor) : null };
}

function decode(s: string): string {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}
