import { Fragment, useMemo } from "react";
import { openExternal } from "@/features/accounts/manager/openExternal";
import type { ProviderFamily } from "@/ipc";
import { cn } from "@/lib/utils";
import { splitRefs, type TextPart } from "./model";

/**
 * Untrusted provider text (issue body, comment, commit message) rendered as plain text: line
 * breaks kept, `https://` links opened in the browser, `#12` / `!12` turned into buttons when
 * `onRef` is given. Markdown is deliberately not interpreted (no HTML from the provider ever
 * reaches the DOM).
 */
export function SafeText({
  text,
  family,
  onRef,
  className,
}: {
  text: string;
  family: ProviderFamily | null;
  onRef?: (part: Extract<TextPart, { type: "ref" }>) => void;
  className?: string;
}) {
  const parts = useMemo(() => splitRefs(text, family), [text, family]);
  return (
    <span className={cn("break-words whitespace-pre-wrap", className)}>
      {parts.map((p, i) => {
        if (p.type === "text") return <Fragment key={`t-${i}`}>{p.text}</Fragment>;
        if (p.type === "url") {
          return (
            <a
              key={`u-${i}`}
              href={p.url}
              onClick={(e) => {
                e.preventDefault();
                void openExternal(p.url);
              }}
              className="text-primary underline-offset-2 hover:underline"
            >
              {p.url}
            </a>
          );
        }
        if (!onRef) return <Fragment key={`r-${i}`}>{p.raw}</Fragment>;
        return (
          <button
            key={`r-${i}`}
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onRef(p);
            }}
            className="font-medium text-primary underline-offset-2 hover:underline"
          >
            {p.raw}
          </button>
        );
      })}
    </span>
  );
}
