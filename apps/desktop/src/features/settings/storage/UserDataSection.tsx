// Phase 6.2 / 6.5: Settings → Storage → "Your data in the app" (custom themes, notes).
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { NotebookPen, Palette, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import { useCustomThemes } from "@/features/custom-theme/store";
import { setNotes, userDataKey } from "@/features/notes/queries";
import { useNotesUi } from "@/features/notes/store";
import { openMyWork } from "@/features/mywork/store";
import { commands, errorMessage, type CustomTheme } from "@/ipc";
import { formatBytes } from "@/lib/format";
import { queryClient } from "@/lib/queryClient";

type Ask = { kind: "theme"; theme: CustomTheme } | { kind: "allThemes" } | { kind: "allNotes" };

export function UserDataSection({ onLeave }: { onLeave?: () => void }) {
  const { t } = useTranslation();
  const summary = useQuery({
    queryKey: userDataKey,
    queryFn: () => commands.userDataSummary(),
    enabled: isTauri(),
    staleTime: 0,
  });
  const themes = useCustomThemes((s) => s.themes);
  const [ask, setAsk] = useState<Ask | null>(null);
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ tone: "error" | "info"; text: string } | null>(null);
  const s = summary.data;
  /** Typed before every note is deleted (case-insensitive). */
  const confirmWord = t("userData.confirmWord");

  const refresh = () => void queryClient.invalidateQueries({ queryKey: userDataKey });

  const run = async () => {
    if (!ask) return;
    setBusy(true);
    try {
      const store = useCustomThemes.getState();
      if (ask.kind === "theme") {
        await store.remove(ask.theme.id);
        setMessage({ tone: "info", text: t("userData.themeDeleted", { name: ask.theme.name }) });
      } else if (ask.kind === "allThemes") {
        await store.removeAll();
        setMessage({ tone: "info", text: t("userData.themesDeleted") });
      } else {
        setNotes(await commands.notesDeleteAll(true));
        useNotesUi.getState().select(null);
        setMessage({ tone: "info", text: t("userData.notesDeleted") });
      }
    } catch (e) {
      setMessage({ tone: "error", text: t("userData.deleteFailed", { error: errorMessage(e) }) });
    } finally {
      setBusy(false);
      setAsk(null);
      setTyped("");
      refresh();
    }
  };

  const openNotes = () => {
    onLeave?.();
    useNotesUi.getState().setTab("notes");
    openMyWork();
  };

  const openThemes = () => {
    onLeave?.();
    useCustomThemes.getState().openManager();
  };

  const notesMissingConfirm =
    ask?.kind === "allNotes" && typed.trim().toUpperCase() !== confirmWord.toUpperCase();

  return (
    <div className="space-y-2 rounded-md border p-3 text-xs" data-testid="user-data">
      <div className="space-y-0.5">
        <div className="font-medium">{t("userData.title")}</div>
        <div className="text-muted-foreground">{t("userData.hint")}</div>
      </div>

      {message && (
        <p
          role={message.tone === "error" ? "alert" : "status"}
          className={message.tone === "error" ? "text-destructive" : "text-muted-foreground"}
        >
          {message.text}
        </p>
      )}

      {/* Custom themes */}
      <div className="space-y-1.5 border-t pt-2">
        <div className="flex flex-wrap items-center gap-2">
          <Palette className="size-3.5 text-muted-foreground" />
          <span className="flex-1">
            {t("userData.themes", { count: themes.length })}
            {s && s.themesBytes > 0 && (
              <span className="text-muted-foreground"> · {formatBytes(s.themesBytes)}</span>
            )}
          </span>
          <Button size="xs" variant="outline" onClick={openThemes}>
            {t("userData.openThemes")}
          </Button>
          <Button
            size="xs"
            variant="outline"
            className="text-destructive"
            disabled={themes.length === 0 || busy}
            onClick={() => setAsk({ kind: "allThemes" })}
          >
            <Trash2 />
            {t("userData.deleteAll")}
          </Button>
        </div>
        {themes.length > 0 && (
          <ul className="space-y-0.5 pl-5">
            {themes.map((theme) => (
              <li key={`ud-${theme.id}`} className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate">{theme.name}</span>
                <Button
                  size="icon-xs"
                  variant="ghost"
                  className="text-destructive"
                  aria-label={t("userData.deleteTheme", { name: theme.name })}
                  disabled={busy}
                  onClick={() => setAsk({ kind: "theme", theme })}
                >
                  <Trash2 />
                </Button>
              </li>
            ))}
          </ul>
        )}
      </div>

      {/* Notes */}
      <div className="flex flex-wrap items-center gap-2 border-t pt-2">
        <NotebookPen className="size-3.5 text-muted-foreground" />
        <span className="flex-1">
          {s
            ? t("userData.notes", {
                notes: s.notes,
                tags: s.userTags,
                size: formatBytes(s.notesBytes),
              })
            : summary.isError
              ? t("userData.loadFailed")
              : t("userData.loading")}
        </span>
        <Button size="xs" variant="outline" onClick={openNotes}>
          {t("userData.openNotes")}
        </Button>
        <Button
          size="xs"
          variant="outline"
          className="text-destructive"
          disabled={!s || (s.notes === 0 && s.userTags === 0) || busy}
          onClick={() => setAsk({ kind: "allNotes" })}
        >
          <Trash2 />
          {t("userData.deleteAll")}
        </Button>
      </div>
      <p className="text-[11px] text-muted-foreground">{t("userData.exportFirst")}</p>

      <ConfirmDialog
        open={ask !== null}
        title={
          ask?.kind === "theme"
            ? t("userData.deleteThemeTitle", { name: ask.theme.name })
            : ask?.kind === "allThemes"
              ? t("userData.deleteThemesTitle", { count: themes.length })
              : t("userData.deleteNotesTitle")
        }
        description={
          ask?.kind === "allNotes"
            ? t("userData.deleteNotesBody", { notes: s?.notes ?? 0, tags: s?.userTags ?? 0 })
            : t("userData.deleteThemesBody")
        }
        confirmLabel={t("userData.deleteConfirm")}
        destructive
        busy={busy || notesMissingConfirm}
        onConfirm={() => void run()}
        onCancel={() => {
          setAsk(null);
          setTyped("");
        }}
      >
        {ask?.kind === "allNotes" && (
          <label className="grid gap-1 text-xs">
            <span>{t("userData.typeToConfirm", { word: confirmWord })}</span>
            <input
              value={typed}
              onChange={(e) => setTyped(e.target.value)}
              aria-label={t("userData.typeToConfirm", { word: confirmWord })}
              className="h-8 rounded-md border bg-background px-2 font-mono text-xs focus:border-destructive focus:outline-none"
            />
          </label>
        )}
      </ConfirmDialog>
    </div>
  );
}
