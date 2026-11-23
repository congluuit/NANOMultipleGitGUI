import { Fragment, useMemo, useState, type MouseEvent, type ReactNode } from "react";
import { Image as ImageIcon, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { openExternal } from "@/features/accounts/manager/openExternal";
import { cn } from "@/lib/utils";
import { parseMarkdown, type Block, type Inline } from "./markdown";
import { formatJson, linkTarget, svgImageUrl, type PreviewKind } from "./model";

interface Props {
  kind: PreviewKind;
  text: string;
  /** Repository path of the file (relative links of a Markdown file start there). */
  path: string;
  /** Opens another file of the repository (a relative Markdown link). */
  onOpenFile?: (path: string) => void;
}

/** The rendered view of the editor's "Preview" mode. */
export function FilePreview({ kind, text, path, onOpenFile }: Props) {
  return (
    <div className="min-h-0 flex-1 overflow-auto bg-background" data-testid="file-preview">
      {kind === "markdown" && <MarkdownPreview text={text} path={path} onOpenFile={onOpenFile} />}
      {kind === "svg" && <SvgPreview text={text} path={path} />}
      {kind === "json" && <JsonPreview text={text} />}
    </div>
  );
}

// ---------------------------------------------------------------------------------------------
// Markdown.

interface MarkdownProps {
  text: string;
  path: string;
  onOpenFile?: (path: string) => void;
}

export function MarkdownPreview({ text, path, onOpenFile }: MarkdownProps) {
  const { t } = useTranslation();
  const blocks = useMemo(() => parseMarkdown(text), [text]);
  const ctx: RenderContext = {
    followable: (href) => linkTarget(href, path).kind !== "none",
    task: (done) => t(done ? "editor.preview.taskDone" : "editor.preview.taskOpen"),
  };
  // Every link is followed here: a web page in the browser, another file in the editor, a
  // heading of this file by scrolling to it. The page itself never navigates.
  const onClick = (e: MouseEvent<HTMLDivElement>) => {
    const link = e.target instanceof Element ? e.target.closest("a") : null;
    if (!link) return;
    e.preventDefault();
    const href = link.dataset.href;
    if (!href) return;
    const target = linkTarget(href, path);
    if (target.kind === "external") void openExternal(target.url);
    else if (target.kind === "file") onOpenFile?.(target.path);
    else if (target.kind === "anchor") {
      const id = target.id.toLowerCase();
      const heading = [...e.currentTarget.querySelectorAll<HTMLElement>("[data-anchor]")].find(
        (h) => h.dataset.anchor === id,
      );
      heading?.scrollIntoView({ block: "start" });
    }
  };
  return (
    // Clicks on the links (focusable anchors: Enter clicks them too) are handled here.
    <div
      onClick={onClick}
      className="mx-auto grid max-w-3xl gap-3 px-6 py-5 text-sm leading-relaxed break-words text-foreground"
      data-testid="markdown-preview"
    >
      {renderBlocks(blocks, ctx)}
    </div>
  );
}

interface RenderContext {
  /** The link leads somewhere the app follows (not `http:`, `javascript:`...). */
  followable: (href: string) => boolean;
  /** Accessible name of a task list checkbox. */
  task: (done: boolean) => string;
}

const HEADING_CLASS = [
  "",
  "border-b pb-1 text-2xl font-semibold",
  "border-b pb-1 text-xl font-semibold",
  "text-lg font-semibold",
  "text-base font-semibold",
  "text-sm font-semibold",
  "text-sm font-semibold text-muted-foreground",
];

function renderBlocks(blocks: Block[], ctx: RenderContext): ReactNode[] {
  return blocks.map((b, i) => renderBlock(b, i, ctx));
}

function renderBlock(b: Block, key: number, ctx: RenderContext): ReactNode {
  switch (b.t) {
    case "heading": {
      const Tag = `h${Math.min(Math.max(b.level, 1), 6)}` as "h1";
      return (
        <Tag key={key} data-anchor={b.id} className={cn("mt-2", HEADING_CLASS[b.level])}>
          {renderInlines(b.c, ctx)}
        </Tag>
      );
    }
    case "p":
      return <p key={key}>{renderInlines(b.c, ctx)}</p>;
    case "code":
      return (
        <pre
          key={key}
          className="overflow-x-auto rounded-md bg-surface px-3 py-2 font-mono text-xs leading-normal"
          data-lang={b.lang || undefined}
        >
          <code>{b.v}</code>
        </pre>
      );
    case "quote":
      return (
        <blockquote
          key={key}
          className="grid gap-2 border-l-4 border-border pl-3 text-muted-foreground"
        >
          {renderBlocks(b.c, ctx)}
        </blockquote>
      );
    case "list": {
      const gap = b.tight ? "gap-0.5" : "gap-2";
      // The item stays a list item (its bullet); its blocks are laid out inside.
      const items = b.items.map((item, i) => (
        <li key={i} className={cn(item.checked !== null && "-ml-5 list-none")}>
          {item.checked === null ? (
            <div className={cn("grid", gap)}>{renderBlocks(item.c, ctx)}</div>
          ) : (
            <div className="flex items-start gap-2">
              <input
                type="checkbox"
                checked={item.checked}
                readOnly
                disabled
                className="mt-1"
                aria-label={ctx.task(item.checked)}
              />
              <div className={cn("grid flex-1", gap)}>{renderBlocks(item.c, ctx)}</div>
            </div>
          )}
        </li>
      ));
      const cls = cn("grid pl-6", gap);
      return b.ordered ? (
        <ol key={key} start={b.start} className={cn(cls, "list-decimal")}>
          {items}
        </ol>
      ) : (
        <ul key={key} className={cn(cls, "list-disc")}>
          {items}
        </ul>
      );
    }
    case "hr":
      return <hr key={key} className="border-border" />;
    case "table":
      return (
        <div key={key} className="overflow-x-auto">
          <table className="border-collapse text-xs">
            <thead>
              <tr>
                {b.head.map((cell, i) => (
                  <th
                    key={i}
                    className="border bg-surface px-2 py-1 font-semibold"
                    style={{ textAlign: b.align[i] ?? "left" }}
                  >
                    {renderInlines(cell, ctx)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {b.rows.map((row, r) => (
                <tr key={r}>
                  {row.map((cell, i) => (
                    <td
                      key={i}
                      className="border px-2 py-1"
                      style={{ textAlign: b.align[i] ?? "left" }}
                    >
                      {renderInlines(cell, ctx)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case "html":
      // Shown as the text it is: HTML in a file never runs in the app.
      return (
        <pre
          key={key}
          className="overflow-x-auto rounded-md border border-dashed px-3 py-2 font-mono text-xs text-muted-foreground"
        >
          {b.v}
        </pre>
      );
  }
}

function renderInlines(inlines: Inline[], ctx: RenderContext): ReactNode[] {
  return inlines.map((n, i) => {
    switch (n.t) {
      case "text":
        return <Fragment key={i}>{n.v}</Fragment>;
      case "code":
        return (
          <code key={i} className="rounded bg-surface px-1 py-0.5 font-mono text-[0.85em]">
            {n.v}
          </code>
        );
      case "strong":
        return <strong key={i}>{renderInlines(n.c, ctx)}</strong>;
      case "em":
        return <em key={i}>{renderInlines(n.c, ctx)}</em>;
      case "del":
        return <del key={i}>{renderInlines(n.c, ctx)}</del>;
      case "br":
        return <br key={i} />;
      case "link": {
        // No real `href`: the preview follows the link itself (see MarkdownPreview).
        const ok = ctx.followable(n.href);
        return (
          <a
            key={i}
            href={ok ? "#" : undefined}
            data-href={ok ? n.href : undefined}
            title={n.href}
            className={cn(ok && "text-primary underline-offset-2 hover:underline")}
          >
            {renderInlines(n.c, ctx)}
          </a>
        );
      }
      case "image":
        // Images are not loaded (no requests to other sites, nothing read from disk).
        return (
          <span
            key={i}
            title={n.src}
            className="inline-flex items-center gap-1 rounded border border-dashed px-1.5 text-xs text-muted-foreground"
          >
            <ImageIcon className="size-3" />
            {n.alt || n.src}
          </span>
        );
    }
  });
}

// ---------------------------------------------------------------------------------------------
// SVG and JSON.

export function SvgPreview({ text, path }: { text: string; path: string }) {
  const { t } = useTranslation();
  const src = useMemo(() => svgImageUrl(text), [text]);
  // The image that failed to draw (a new text gets a new chance).
  const [failed, setFailed] = useState<string | null>(null);
  const name = path.split("/").pop() ?? path;
  return (
    <div className="flex min-h-full items-center justify-center p-6">
      {failed === src ? (
        <PreviewError message={t("editor.preview.svgInvalid")} />
      ) : (
        <img
          src={src}
          alt={name}
          onError={() => setFailed(src)}
          className="max-h-full max-w-full rounded border bg-[repeating-conic-gradient(var(--surface)_0_25%,transparent_0_50%)] bg-[length:16px_16px] object-contain"
          data-testid="svg-preview"
        />
      )}
    </div>
  );
}

export function JsonPreview({ text }: { text: string }) {
  const { t } = useTranslation();
  const result = useMemo(() => formatJson(text), [text]);
  if (!result.ok) {
    return (
      <div className="p-4">
        <PreviewError
          message={
            result.line !== null
              ? t("editor.preview.jsonInvalidAt", { line: result.line, column: result.column })
              : t("editor.preview.jsonInvalid")
          }
          detail={result.message}
        />
      </div>
    );
  }
  return (
    <pre className="px-4 py-3 font-mono text-xs leading-normal" data-testid="json-preview">
      {result.pretty}
    </pre>
  );
}

function PreviewError({ message, detail }: { message: string; detail?: string }) {
  return (
    <div
      role="alert"
      className="flex items-start gap-2 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-xs"
    >
      <TriangleAlert className="mt-0.5 size-4 shrink-0 text-danger" />
      <div className="grid gap-0.5">
        <span>{message}</span>
        {detail && <span className="font-mono text-[11px] text-muted-foreground">{detail}</span>}
      </div>
    </div>
  );
}
