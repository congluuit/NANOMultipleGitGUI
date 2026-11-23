import { TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { LineEnding, TextEncoding } from "@/ipc";
import { cn } from "@/lib/utils";
import {
  REOPEN_ENCODINGS,
  SAVE_ENCODINGS,
  encodingLabel,
  eolLabel,
  sameReadEncoding,
  type Eol,
} from "./encoding";

const TRIGGER =
  "flex items-center gap-1 rounded px-1 hover:bg-surface-2 hover:text-foreground focus-visible:outline-1 focus-visible:outline-ring";
const ITEM = "text-xs";

interface EncodingMenuProps {
  /** Encoding of the text shown (the one the next save writes). */
  encoding: TextEncoding;
  /** Reading the file again is possible (no unsaved edits to lose). */
  canReopen: boolean;
  /** The text can be saved (an editable file). */
  canSave: boolean;
  onReopen: (encoding: TextEncoding | null) => void;
  onSave: (encoding: TextEncoding) => void;
}

/** The encoding in the editor's status bar, with "Reopen with encoding" and "Save with encoding". */
export function EncodingMenu({
  encoding,
  canReopen,
  canSave,
  onReopen,
  onSave,
}: EncodingMenuProps) {
  const { t } = useTranslation();
  const label = encodingLabel(encoding, t);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className={TRIGGER}
        title={t("editor.encoding.title")}
        aria-label={t("editor.encoding.button", { encoding: label })}
        data-testid="encoding-menu"
      >
        {label}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" side="top" className="min-w-56">
        <DropdownMenuGroup aria-label={t("editor.encoding.reopen")}>
          <DropdownMenuLabel className="text-xs">{t("editor.encoding.reopen")}</DropdownMenuLabel>
          {REOPEN_ENCODINGS.map((e) => {
            const current = e !== null && sameReadEncoding(e, encoding);
            return (
              <DropdownMenuItem
                key={e ?? "auto"}
                className={cn(ITEM, current && "font-semibold")}
                disabled={!canReopen}
                data-current={current || undefined}
                onSelect={() => onReopen(e)}
              >
                {e === null ? t("editor.encoding.detect") : encodingLabel(e, t)}
              </DropdownMenuItem>
            );
          })}
          {!canReopen && (
            <p className="px-2 pb-1 text-[11px] text-muted-foreground">
              {t("editor.encoding.reopenDirty")}
            </p>
          )}
        </DropdownMenuGroup>
        {canSave && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuGroup aria-label={t("editor.encoding.save")}>
              <DropdownMenuLabel className="text-xs">{t("editor.encoding.save")}</DropdownMenuLabel>
              {SAVE_ENCODINGS.map((e) => (
                <DropdownMenuItem
                  key={e}
                  className={cn(ITEM, e === encoding && "font-semibold")}
                  data-current={e === encoding || undefined}
                  onSelect={() => onSave(e)}
                >
                  {encodingLabel(e, t)}
                </DropdownMenuItem>
              ))}
            </DropdownMenuGroup>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

interface EolMenuProps {
  /** Line endings of the text shown: `mixed` until one is picked. */
  eol: LineEnding;
  /** Converting is possible (an editable file). */
  canConvert: boolean;
  onConvert: (eol: Eol) => void;
}

/** The line endings in the editor's status bar, with converting to LF or CRLF. */
export function EolMenu({ eol, canConvert, onConvert }: EolMenuProps) {
  const { t } = useTranslation();
  const label = eolLabel(eol, t);
  const mixed = eol === "mixed";
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className={cn(TRIGGER, mixed && "text-warning")}
        disabled={!canConvert}
        title={mixed ? t("editor.eol.mixedTitle") : t("editor.eol.title")}
        aria-label={t("editor.eol.button", { eol: label })}
        data-testid="eol-menu"
      >
        {mixed && <TriangleAlert className="size-3" />}
        {label}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" side="top" className="min-w-56">
        <DropdownMenuLabel className="text-xs">{t("editor.eol.convert")}</DropdownMenuLabel>
        {(["lf", "crlf"] as const).map((e) => (
          <DropdownMenuItem
            key={e}
            className={cn(ITEM, e === eol && "font-semibold")}
            data-current={e === eol || undefined}
            onSelect={() => onConvert(e)}
          >
            {t(e === "lf" ? "editor.eol.toLf" : "editor.eol.toCrlf")}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
