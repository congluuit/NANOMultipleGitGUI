import { Fragment, type ReactNode } from "react";

/**
 * The few bits of Markdown models use in short answers: ``` blocks, `code`, **bold** and
 * numbered / dashed lists (kept as plain lines). Rendered as React text, never as HTML.
 */
export function Answer({ text }: { text: string }) {
  const parts = text.split(/```[^\n]*\n?/);
  return (
    <div className="grid gap-2 text-xs leading-relaxed">
      {parts.map((part, i) =>
        i % 2 === 1 ? (
          <pre
            key={i}
            className="overflow-x-auto rounded bg-muted px-2 py-1.5 font-mono text-[11px] whitespace-pre-wrap"
          >
            {part.replace(/\n$/, "")}
          </pre>
        ) : (
          <Paragraphs key={i} text={part} />
        ),
      )}
    </div>
  );
}

function Paragraphs({ text }: { text: string }) {
  const blocks = text
    .split(/\n{2,}/)
    .map((b) => b.trim())
    .filter(Boolean);
  return (
    <>
      {blocks.map((b, i) => (
        <p key={i} className="break-words whitespace-pre-line">
          {inline(b.replace(/^#{1,6}\s+/gm, ""))}
        </p>
      ))}
    </>
  );
}

function inline(text: string): ReactNode[] {
  return text.split(/(`[^`\n]+`|\*\*[^*\n]+\*\*)/).map((piece, i) => {
    if (piece.startsWith("`") && piece.endsWith("`") && piece.length > 2) {
      return (
        <code key={i} className="rounded bg-muted px-1 font-mono text-[11px]">
          {piece.slice(1, -1)}
        </code>
      );
    }
    if (piece.startsWith("**") && piece.endsWith("**") && piece.length > 4) {
      return <strong key={i}>{piece.slice(2, -2)}</strong>;
    }
    return <Fragment key={i}>{piece}</Fragment>;
  });
}
