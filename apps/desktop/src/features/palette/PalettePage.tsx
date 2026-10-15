import { useLayoutEffect, useRef, useState } from "react";
import { ArrowLeft, Check, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import { useUi } from "@/stores/ui";
import { expandHex } from "@/theme/color";
import { CONTRAST_RULES, contrastRatio } from "@/theme/contrast";
import type { ResolvedTheme } from "@/theme/theme";
import { COLOR_TOKENS, LANE_TOKENS, type TokenMap } from "@/theme/tokens";

function useTokenValues(ref: React.RefObject<HTMLElement | null>): TokenMap {
  const [values, setValues] = useState<TokenMap>({});
  useLayoutEffect(() => {
    if (!ref.current) return;
    const style = getComputedStyle(ref.current);
    const map: TokenMap = {};
    for (const token of [...COLOR_TOKENS, ...LANE_TOKENS]) {
      map[token] = expandHex(style.getPropertyValue(token).trim());
    }
    setValues(map);
  }, [ref]);
  return values;
}

function ThemeColumn({ theme }: { theme: ResolvedTheme }) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const values = useTokenValues(ref);

  return (
    <div
      ref={ref}
      data-theme={theme}
      className="flex-1 space-y-6 rounded-lg border bg-background p-5 text-foreground"
    >
      <h2 className="text-sm font-bold uppercase">{theme}</h2>

      <section className="space-y-2">
        <h3 className="text-xs font-semibold text-muted-foreground">{t("palette.tokens")}</h3>
        <div className="grid grid-cols-2 gap-2">
          {COLOR_TOKENS.map((token) => (
            <div key={token} className="flex items-center gap-2 rounded-md border bg-surface p-1.5">
              <span
                className="size-7 shrink-0 rounded border"
                style={{ background: `var(${token})` }}
              />
              <span className="min-w-0 text-[11px] leading-tight">
                <span className="block font-mono">{token}</span>
                <span className="block font-mono text-muted-foreground">{values[token]}</span>
              </span>
            </div>
          ))}
        </div>
      </section>

      <section className="space-y-2">
        <h3 className="text-xs font-semibold text-muted-foreground">{t("palette.lanes")}</h3>
        <div className="flex gap-2 rounded-md border bg-surface p-3">
          {LANE_TOKENS.map((token) => (
            <span key={token} className="flex flex-1 flex-col items-center gap-1">
              <span className="h-10 w-1 rounded" style={{ background: `var(${token})` }} />
              <span
                className="size-3 rounded-full border-2"
                style={{ borderColor: `var(${token})`, background: "var(--surface)" }}
              />
              <span className="font-mono text-[9px] text-muted-foreground">{values[token]}</span>
            </span>
          ))}
        </div>
      </section>

      <section className="space-y-2">
        <h3 className="text-xs font-semibold text-muted-foreground">{t("palette.sample")}</h3>
        <div className="space-y-3 rounded-md border bg-surface p-3">
          <div className="flex flex-wrap gap-2">
            <Button size="sm">Commit</Button>
            <Button size="sm" variant="outline">
              Fetch
            </Button>
            <Button size="sm" variant="secondary">
              Stash
            </Button>
            <Button size="sm" variant="destructive">
              Discard
            </Button>
          </div>
          <p className="text-xs">
            Primary text · <span className="text-muted-foreground">muted text</span> ·{" "}
            <span className="text-primary">primary link</span>
          </p>
          <div className="flex flex-wrap gap-3 text-xs font-medium">
            <span className="text-success">✓ success</span>
            <span className="text-warning">⚠ warning</span>
            <span className="text-danger">✕ danger</span>
            <span className="text-info">ℹ info</span>
          </div>
        </div>
      </section>

      <section className="space-y-2">
        <h3 className="text-xs font-semibold text-muted-foreground">{t("palette.contrast")}</h3>
        <table className="w-full text-[11px]">
          <tbody>
            {CONTRAST_RULES.map((rule) => {
              const fg = values[rule.fg];
              const bg = values[rule.bg];
              if (!fg || !bg) return null;
              // An unexpected color format must not take the whole page down.
              let ratio = Number.NaN;
              try {
                ratio = contrastRatio(fg, bg);
              } catch {
                /* shown as "—" */
              }
              const pass = ratio >= rule.min;
              return (
                <tr key={`${rule.fg}-${rule.bg}`} className="border-b last:border-0">
                  <td className="py-0.5 font-mono">
                    {rule.fg} / {rule.bg}
                  </td>
                  <td className="text-right tabular-nums">
                    {Number.isNaN(ratio) ? "—" : ratio.toFixed(2)}
                  </td>
                  <td className="text-right text-muted-foreground">≥ {rule.min}</td>
                  <td className={cn("pl-2 text-right", pass ? "text-success" : "text-danger")}>
                    {pass ? <Check className="inline size-3" /> : <X className="inline size-3" />}{" "}
                    {pass ? t("palette.pass") : t("palette.fail")}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </section>
    </div>
  );
}

export function PalettePage() {
  const { t } = useTranslation();
  const setView = useUi((s) => s.setView);
  return (
    <div className="flex h-full flex-col">
      <header className="flex h-12 shrink-0 items-center gap-3 border-b bg-surface px-3">
        <Button variant="ghost" size="sm" onClick={() => setView("workspace")}>
          <ArrowLeft />
          {t("palette.back")}
        </Button>
        <h1 className="text-sm font-semibold">{t("palette.title")}</h1>
      </header>
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex gap-4 p-4">
          <ThemeColumn theme="light" />
          <ThemeColumn theme="dark" />
        </div>
      </ScrollArea>
    </div>
  );
}
