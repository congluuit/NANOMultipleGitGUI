/** Phase 6.7: "Hướng dẫn đầy đủ" — topics on the left (with search), content on the right. */
import { Fragment, useState } from "react";
import {
  BookOpen,
  ChevronLeft,
  ChevronRight,
  Info,
  Lightbulb,
  Search,
  TriangleAlert,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { guideFor, searchGuide, type GuideBlock, type GuideTopic } from "./content";
import { useGuide } from "./store";
// Registers the palette command "Open the full guide".
import "./palette";

/** Mounted once (App). */
export function GuideHost() {
  const open = useGuide((s) => s.open);
  const topic = useGuide((s) => s.topic);
  const close = useGuide((s) => s.close);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent
        showCloseButton={false}
        className="h-[min(48rem,calc(100dvh-2rem))] gap-0 overflow-hidden p-0 sm:max-w-5xl"
      >
        {/* Fresh search / selection every time it opens. */}
        {open && <GuideBody key={topic ?? "first"} initial={topic} onClose={close} />}
      </DialogContent>
    </Dialog>
  );
}

function GuideBody({ initial, onClose }: { initial: string | null; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const topics = guideFor(i18n.language);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<string | null>(initial);
  const results = searchGuide(topics, query);
  const current =
    results.find((x) => x.id === selected) ?? results[0] ?? (query ? null : topics[0]!);
  const index = current ? topics.findIndex((x) => x.id === current.id) : -1;
  const go = (topic: GuideTopic | undefined) => {
    if (!topic) return;
    setQuery("");
    setSelected(topic.id);
  };

  return (
    <div className="flex h-full min-h-0">
      <nav
        aria-label={t("guide.topics")}
        className="flex w-60 shrink-0 flex-col border-r bg-surface"
      >
        <div className="space-y-2 p-3">
          <DialogTitle className="flex items-center gap-2 text-sm">
            <BookOpen className="size-4 text-primary" />
            {t("guide.title")}
          </DialogTitle>
          <DialogDescription className="text-[11px]">{t("guide.description")}</DialogDescription>
          <label className="flex h-8 items-center gap-1.5 rounded-md border bg-background px-2 focus-within:border-primary">
            <Search className="size-3.5 shrink-0 text-muted-foreground" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("guide.search")}
              aria-label={t("guide.search")}
              className="h-full min-w-0 flex-1 bg-transparent text-xs placeholder:text-muted-foreground focus:outline-none"
            />
          </label>
          {query && (
            <p className="text-[11px] text-muted-foreground" role="status">
              {results.length
                ? t("guide.results", { count: results.length })
                : t("guide.noResults")}
            </p>
          )}
        </div>
        <ul className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
          {results.map((topic) => (
            <li key={topic.id}>
              <button
                type="button"
                onClick={() => setSelected(topic.id)}
                aria-current={topic.id === current?.id ? "page" : undefined}
                className={cn(
                  "w-full rounded-md px-2 py-1.5 text-left text-xs hover:bg-surface-2",
                  topic.id === current?.id && "bg-surface-2 font-semibold text-primary",
                )}
              >
                {topic.title}
              </button>
            </li>
          ))}
        </ul>
      </nav>

      <div className="relative flex min-w-0 flex-1 flex-col">
        <Button
          variant="ghost"
          size="icon"
          className="absolute top-2 right-2 z-10 size-7"
          aria-label={t("common.close")}
          onClick={onClose}
        >
          <X />
        </Button>
        {current ? (
          <article
            key={current.id}
            className="min-h-0 flex-1 overflow-y-auto px-8 py-6 text-sm leading-relaxed"
          >
            <header className="mb-5 pr-8">
              <h2 className="text-lg font-semibold">{current.title}</h2>
              <p className="text-muted-foreground">{current.summary}</p>
            </header>
            {current.sections.map((s) => (
              <section key={s.id} className="mb-6">
                <h3 className="mb-2 text-sm font-semibold">{s.title}</h3>
                <div className="space-y-3">
                  {s.blocks.map((b, i) => (
                    <Block key={`${s.id}-${i}`} block={b} />
                  ))}
                </div>
              </section>
            ))}
            <footer className="mt-8 flex justify-between gap-2 border-t pt-4">
              <Button
                variant="outline"
                size="sm"
                disabled={index <= 0}
                onClick={() => go(topics[index - 1])}
              >
                <ChevronLeft />
                {index > 0 ? topics[index - 1]!.title : t("guide.prev")}
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={index < 0 || index >= topics.length - 1}
                onClick={() => go(topics[index + 1])}
              >
                {index >= 0 && index < topics.length - 1
                  ? topics[index + 1]!.title
                  : t("guide.next")}
                <ChevronRight />
              </Button>
            </footer>
          </article>
        ) : (
          <p className="p-8 text-sm text-muted-foreground">{t("guide.noResults")}</p>
        )}
      </div>
    </div>
  );
}

/** `**bold**` → <strong>. */
function Rich({ text }: { text: string }) {
  const parts = text.split("**");
  return (
    <>
      {parts.map((p, i) =>
        i % 2 === 1 ? (
          <strong key={i} className="font-semibold">
            {p}
          </strong>
        ) : (
          <Fragment key={i}>{p}</Fragment>
        ),
      )}
    </>
  );
}

function Block({ block: b }: { block: GuideBlock }) {
  const { t } = useTranslation();
  switch (b.kind) {
    case "p":
      return (
        <p>
          <Rich text={b.text} />
        </p>
      );
    case "steps":
      return (
        <ol className="list-decimal space-y-1.5 pl-5 marker:font-semibold marker:text-primary">
          {b.items.map((item, i) => (
            <li key={i}>
              <Rich text={item} />
            </li>
          ))}
        </ol>
      );
    case "list":
      return (
        <ul className="list-disc space-y-1.5 pl-5 marker:text-muted-foreground">
          {b.items.map((item, i) => (
            <li key={i}>
              <Rich text={item} />
            </li>
          ))}
        </ul>
      );
    case "term":
      return (
        <p className="flex gap-2 rounded-md border bg-surface px-3 py-2 text-xs">
          <Info className="mt-0.5 size-3.5 shrink-0 text-info" />
          <span>
            <span className="font-semibold">{b.term}</span>
            {t("guide.termSeparator")}
            <Rich text={b.text} />
          </span>
        </p>
      );
    case "tip":
      return (
        <p className="flex gap-2 rounded-md bg-success/10 px-3 py-2 text-xs">
          <Lightbulb className="mt-0.5 size-3.5 shrink-0 text-success" />
          <span>
            <Rich text={b.text} />
          </span>
        </p>
      );
    case "warn":
      return (
        <p className="flex gap-2 rounded-md bg-warning/10 px-3 py-2 text-xs">
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-warning" />
          <span>
            <Rich text={b.text} />
          </span>
        </p>
      );
    case "keys":
      return (
        <table className="w-full text-xs">
          <tbody>
            {b.rows.map(([keys, action]) => (
              <tr key={keys} className="border-b last:border-0">
                <td className="w-56 py-1.5 pr-3 align-top">
                  <kbd className="rounded border bg-surface px-1.5 py-0.5 font-mono text-[11px]">
                    {keys}
                  </kbd>
                </td>
                <td className="py-1.5">{action}</td>
              </tr>
            ))}
          </tbody>
        </table>
      );
  }
}
