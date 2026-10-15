/**
 * Release notes as plain text. They come from the release body (Markdown typed by the
 * maintainer) and are rendered with React text nodes only — never as HTML. Template comments
 * (`<!-- … -->`) are dropped and long runs of blank lines collapsed.
 */
export function cleanNotes(notes: string | null | undefined): string {
  if (!notes) return "";
  return notes
    .replace(/\r\n?/g, "\n")
    .replace(/<!--[\s\S]*?(?:-->|$)/g, "")
    .replace(/[ \t]+$/gm, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

/** `1.2.3` → `v1.2.3` (leaves an existing prefix alone). */
export function versionLabel(version: string): string {
  return /^v/i.test(version) ? version : `v${version}`;
}
