// Phase 6.2: theme manager — list, create / duplicate / delete, import / export, color editor
// with live preview and contrast warnings.
import { useEffect, useState } from "react";
import {
  Check,
  ChevronDown,
  Copy,
  Download,
  Lock,
  Plus,
  Save,
  Trash2,
  TriangleAlert,
  Undo2,
  Upload,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { commands, errorMessage, type CustomTheme } from "@/ipc";
import { cn } from "@/lib/utils";
import { fileProblemText, pickOpenFile, pickSaveFile, safeFileName } from "@/lib/userFiles";
import { useSettings } from "@/stores/settings";
import type { ResolvedTheme } from "@/theme/theme";
import {
  contrastIssues,
  findTheme,
  formatRatio,
  isPresetId,
  normalizeHex,
  PRESET_IDS,
  sourceColors,
  TOKEN_GROUPS,
  tokenKey,
  uniqueName,
  type ThemeColors,
  type ThemeDef,
  type ThemeSource,
  type ThemeToken,
} from "./model";
import { useCustomThemes } from "./store";

interface Draft {
  name: string;
  base: ResolvedTheme;
  colors: ThemeColors;
}

interface Message {
  tone: "error" | "info";
  text: string;
  detail?: string;
}

/** Mounted once (App). */
export function ThemeManagerHost() {
  const open = useCustomThemes((s) => s.managerOpen);
  return open ? <ThemeManager /> : null;
}

function draftOf(def: ThemeDef | null): Draft | null {
  return def ? { name: def.name, base: def.base, colors: { ...def.colors } } : null;
}

function sameDraft(a: Draft | null, b: Draft | null): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function ThemeManager() {
  const { t } = useTranslation();
  const themes = useCustomThemes((s) => s.themes);
  const activeId = useCustomThemes((s) => s.activeId);
  const focus = useCustomThemes((s) => s.managerFocus);
  const mode = useSettings((s) => s.theme);
  const store = useCustomThemes.getState;

  const [selectedId, setSelectedId] = useState<string>(
    () =>
      (findTheme(focus, themes) ? focus : findTheme(activeId, themes) ? activeId : null) ??
      "hc-dark",
  );
  const [draft, setDraft] = useState<Draft | null>(() => draftOf(findTheme(selectedId, themes)));
  const [previewOn, setPreviewOn] = useState(true);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<Message | null>(null);
  /** Waiting for "discard unsaved changes?" before running this. */
  const [pending, setPending] = useState<(() => void) | null>(null);
  const [deleteAsk, setDeleteAsk] = useState(false);

  const def = findTheme(selectedId, themes);
  const saved = draftOf(def);
  const dirty = Boolean(def && !def.preset && !sameDraft(draft, saved));
  const nameOf = (d: ThemeDef | null, id = d?.id) =>
    isPresetId(id) ? t(`customTheme.preset.${id}`) : (d?.name ?? "");

  useEffect(() => {
    store().setPreview(previewOn && draft ? { base: draft.base, colors: draft.colors } : null);
  }, [previewOn, draft, store]);
  useEffect(() => () => store().setPreview(null), [store]);

  const guard = (run: () => void) => (dirty ? setPending(() => run) : run());

  const select = (id: string, list: readonly CustomTheme[] = themes) => {
    setSelectedId(id);
    setDraft(draftOf(findTheme(id, list)));
    setMessage(null);
  };

  const close = () => guard(() => store().closeManager());

  const fail = (text: string, e: unknown) =>
    setMessage({ tone: "error", text, detail: errorMessage(e) });

  const create = async (source: ThemeSource, name: string) => {
    setBusy(true);
    try {
      const { base, colors } = sourceColors(source, themes);
      const theme = await store().save({
        name: uniqueName(
          name,
          themes.map((x) => x.name),
        ),
        base,
        colors,
      });
      select(theme.id, store().themes);
      setMessage({ tone: "info", text: t("customTheme.created", { name: theme.name }) });
    } catch (e) {
      fail(t("customTheme.saveFailed"), e);
    } finally {
      setBusy(false);
    }
  };

  const save = async (): Promise<boolean> => {
    if (!def || def.preset || !draft) return false;
    const name = draft.name.trim();
    if (!name) {
      setMessage({ tone: "error", text: t("customTheme.nameRequired") });
      return false;
    }
    setBusy(true);
    try {
      const theme = await store().save({
        id: def.id,
        name,
        base: draft.base,
        colors: draft.colors,
      });
      setDraft(draftOf(findTheme(theme.id, store().themes)));
      setMessage({ tone: "info", text: t("customTheme.saved") });
      return true;
    } catch (e) {
      fail(t("customTheme.saveFailed"), e);
      return false;
    } finally {
      setBusy(false);
    }
  };

  const use = async () => {
    if (!def) return;
    if (dirty && !(await save())) return;
    try {
      await store().activate(def.id);
      setMessage({ tone: "info", text: t("customTheme.activated", { name: nameOf(def) }) });
    } catch (e) {
      fail(t("customTheme.activateFailed"), e);
    }
  };

  const remove = async () => {
    if (!def || def.preset) return;
    setDeleteAsk(false);
    setBusy(true);
    try {
      await store().remove(def.id);
      select("hc-dark", store().themes);
      setMessage({ tone: "info", text: t("customTheme.deleted", { name: def.name }) });
    } catch (e) {
      fail(t("customTheme.deleteFailed"), e);
    } finally {
      setBusy(false);
    }
  };

  const exportTheme = async () => {
    if (!def || !draft) return;
    const name = nameOf(def) || draft.name;
    try {
      const path = await pickSaveFile({
        title: t("customTheme.exportTitle"),
        defaultName: `${safeFileName(name, "theme")}.json`,
        filterName: t("customTheme.fileFilter"),
        extension: "json",
      });
      if (!path) return;
      await commands.customThemeExport(path, { name, base: draft.base, colors: draft.colors });
      setMessage({ tone: "info", text: t("customTheme.exported", { path }) });
    } catch (e) {
      fail(t("customTheme.exportFailed"), e);
    }
  };

  const importTheme = async () => {
    try {
      const path = await pickOpenFile({
        title: t("customTheme.importTitle"),
        filterName: t("customTheme.fileFilter"),
        extension: "json",
      });
      if (!path) return;
      setBusy(true);
      const result = await commands.customThemeImport(path);
      if (result.problem || !result.theme) {
        setMessage({
          tone: "error",
          text: t("customTheme.importFailed"),
          detail: result.problem ? fileProblemText(t, result.problem) : undefined,
        });
        return;
      }
      const theme = result.theme;
      await store().load();
      select(theme.id, store().themes);
      setMessage({ tone: "info", text: t("customTheme.imported", { name: theme.name }) });
    } catch (e) {
      fail(t("customTheme.importFailed"), e);
    } finally {
      setBusy(false);
    }
  };

  const setColor = (token: ThemeToken, value: string) =>
    setDraft((d) => (d ? { ...d, colors: { ...d.colors, [token]: value } } : d));

  const issues = draft ? contrastIssues(draft.colors) : [];
  const readOnly = !def || def.preset;
  const inUse = (id: string) => mode === "custom" && activeId === id;

  return (
    <>
      <Dialog open onOpenChange={(o) => !o && close()}>
        <DialogContent className="flex h-[min(46rem,calc(100dvh-3rem))] flex-col gap-3 sm:max-w-4xl">
          <DialogHeader>
            <DialogTitle>{t("customTheme.title")}</DialogTitle>
            <DialogDescription>{t("customTheme.description")}</DialogDescription>
          </DialogHeader>

          {message && (
            <div
              role={message.tone === "error" ? "alert" : "status"}
              className={cn(
                "rounded-md border p-2 text-xs",
                message.tone === "error"
                  ? "border-destructive/40 bg-destructive/5 text-destructive"
                  : "bg-muted/40",
              )}
            >
              <div className="break-words">{message.text}</div>
              {message.detail && (
                <div className="mt-0.5 break-words text-muted-foreground">{message.detail}</div>
              )}
            </div>
          )}

          <div className="grid min-h-0 flex-1 grid-cols-[13rem_1fr] gap-3">
            {/* List */}
            <div className="flex min-h-0 flex-col gap-2">
              <ScrollArea className="min-h-0 flex-1 rounded-md border">
                <div className="p-1 text-xs">
                  <ListLabel>{t("customTheme.presets")}</ListLabel>
                  {PRESET_IDS.map((id) => (
                    <ThemeRow
                      key={`p-${id}`}
                      def={findTheme(id, themes)!}
                      name={nameOf(null, id)}
                      selected={selectedId === id}
                      inUse={inUse(id)}
                      onClick={() => selectedId !== id && guard(() => select(id))}
                    />
                  ))}
                  <ListLabel>{t("customTheme.yourThemes")}</ListLabel>
                  {themes.length === 0 && (
                    <p className="px-2 py-1 text-[11px] text-muted-foreground">
                      {t("customTheme.noThemes")}
                    </p>
                  )}
                  {themes.map((th) => (
                    <ThemeRow
                      key={`u-${th.id}`}
                      def={findTheme(th.id, themes)!}
                      name={th.name}
                      selected={selectedId === th.id}
                      inUse={inUse(th.id)}
                      onClick={() => selectedId !== th.id && guard(() => select(th.id))}
                    />
                  ))}
                </div>
              </ScrollArea>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button size="sm" className="h-8 text-xs" disabled={busy}>
                    <Plus />
                    {t("customTheme.new")}
                    <ChevronDown />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" className="w-60 text-xs">
                  {(["light", "dark", ...PRESET_IDS] as const).map((source) => (
                    <DropdownMenuItem
                      key={`src-${source}`}
                      className="text-xs"
                      onSelect={() => guard(() => void create(source, t("customTheme.newName")))}
                    >
                      {t("customTheme.newFrom", {
                        name:
                          source === "light"
                            ? t("settings.themeLight")
                            : source === "dark"
                              ? t("settings.themeDark")
                              : nameOf(null, source),
                      })}
                    </DropdownMenuItem>
                  ))}
                  {def && (
                    <>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem
                        className="text-xs"
                        onSelect={() =>
                          guard(
                            () =>
                              void create(
                                { copyOf: def.id },
                                t("customTheme.copyName", { name: nameOf(def) }),
                              ),
                          )
                        }
                      >
                        {t("customTheme.duplicateOf", { name: nameOf(def) })}
                      </DropdownMenuItem>
                    </>
                  )}
                </DropdownMenuContent>
              </DropdownMenu>
              <Button
                size="sm"
                variant="outline"
                className="h-8 text-xs"
                disabled={busy}
                onClick={() => guard(() => void importTheme())}
              >
                <Upload />
                {t("customTheme.import")}
              </Button>
            </div>

            {/* Editor */}
            {def && draft ? (
              <div className="flex min-h-0 flex-col gap-2">
                <ScrollArea className="min-h-0 flex-1 rounded-md border">
                  <div className="space-y-4 p-3 text-xs">
                    <div className="flex flex-wrap items-end gap-3">
                      <label className="grid min-w-48 flex-1 gap-1">
                        <span className="text-muted-foreground">{t("customTheme.name")}</span>
                        {readOnly ? (
                          <span className="flex h-8 items-center gap-1.5 font-medium">
                            <Lock className="size-3.5 text-muted-foreground" />
                            {nameOf(def)}
                          </span>
                        ) : (
                          <input
                            value={draft.name}
                            maxLength={80}
                            onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                            className="h-8 rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none"
                          />
                        )}
                      </label>
                      <div className="grid gap-1">
                        <span className="text-muted-foreground">{t("customTheme.base")}</span>
                        <ToggleGroup
                          type="single"
                          variant="outline"
                          size="sm"
                          value={draft.base}
                          disabled={readOnly}
                          onValueChange={(v) =>
                            v && setDraft({ ...draft, base: v as ResolvedTheme })
                          }
                        >
                          <ToggleGroupItem
                            value="light"
                            className="text-xs data-[state=on]:border-primary data-[state=on]:text-primary"
                          >
                            {t("settings.themeLight")}
                          </ToggleGroupItem>
                          <ToggleGroupItem
                            value="dark"
                            className="text-xs data-[state=on]:border-primary data-[state=on]:text-primary"
                          >
                            {t("settings.themeDark")}
                          </ToggleGroupItem>
                        </ToggleGroup>
                      </div>
                    </div>
                    <p className="text-[11px] text-muted-foreground">
                      {readOnly ? t("customTheme.presetHint") : t("customTheme.baseHint")}
                    </p>

                    <ContrastSummary issues={issues} />

                    {TOKEN_GROUPS.map((group) => (
                      <section key={`g-${group.id}`} className="space-y-1.5">
                        <h3 className="text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
                          {t(`customTheme.group.${group.id}`)}
                        </h3>
                        <div className="grid gap-1.5 sm:grid-cols-2">
                          {group.tokens.map((token) => (
                            <ColorField
                              key={`c-${token}`}
                              token={token}
                              value={draft.colors[token] ?? "#000000"}
                              readOnly={readOnly}
                              warn={issues.some((i) => i.fg === token)}
                              onChange={(v) => setColor(token, v)}
                            />
                          ))}
                        </div>
                      </section>
                    ))}
                  </div>
                </ScrollArea>

                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={previewOn}
                    onChange={(e) => setPreviewOn(e.target.checked)}
                    className="accent-primary"
                  />
                  {t("customTheme.previewToggle")}
                </label>

                <div className="flex flex-wrap items-center gap-2">
                  {!readOnly && (
                    <>
                      <Button
                        size="sm"
                        className="h-8 text-xs"
                        disabled={!dirty || busy}
                        onClick={() => void save()}
                      >
                        <Save />
                        {t("customTheme.save")}
                      </Button>
                      <Button
                        size="sm"
                        variant="outline"
                        className="h-8 text-xs"
                        disabled={!dirty || busy}
                        onClick={() => setDraft(saved)}
                      >
                        <Undo2 />
                        {t("customTheme.discard")}
                      </Button>
                    </>
                  )}
                  <Button
                    size="sm"
                    variant={readOnly ? "default" : "outline"}
                    className="h-8 text-xs"
                    disabled={busy || (inUse(def.id) && !dirty)}
                    onClick={() => void use()}
                  >
                    <Check />
                    {inUse(def.id) && !dirty
                      ? t("customTheme.inUse")
                      : dirty
                        ? t("customTheme.saveAndUse")
                        : t("customTheme.use")}
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    className="h-8 text-xs"
                    disabled={busy}
                    onClick={() =>
                      guard(
                        () =>
                          void create(
                            { copyOf: def.id },
                            t("customTheme.copyName", { name: nameOf(def) }),
                          ),
                      )
                    }
                  >
                    <Copy />
                    {t("customTheme.duplicate")}
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    className="h-8 text-xs"
                    disabled={busy}
                    onClick={() => void exportTheme()}
                  >
                    <Download />
                    {t("customTheme.export")}
                  </Button>
                  {!readOnly && (
                    <Button
                      size="sm"
                      variant="outline"
                      className="h-8 text-xs text-destructive"
                      disabled={busy}
                      onClick={() => setDeleteAsk(true)}
                    >
                      <Trash2 />
                      {t("customTheme.delete")}
                    </Button>
                  )}
                  <span className="flex-1" />
                  <Button size="sm" variant="ghost" className="h-8 text-xs" onClick={close}>
                    {t("customTheme.close")}
                  </Button>
                </div>
              </div>
            ) : (
              <div className="text-xs text-muted-foreground">{t("customTheme.pickTheme")}</div>
            )}
          </div>
        </DialogContent>
      </Dialog>

      <ConfirmDialog
        open={pending !== null}
        title={t("customTheme.unsavedTitle")}
        description={t("customTheme.unsavedBody")}
        confirmLabel={t("customTheme.unsavedDiscard")}
        destructive
        onConfirm={() => {
          const run = pending;
          setPending(null);
          setDraft(saved);
          run?.();
        }}
        onCancel={() => setPending(null)}
      />

      <ConfirmDialog
        open={deleteAsk}
        title={t("customTheme.deleteTitle", { name: def?.name ?? "" })}
        description={
          def && inUse(def.id) ? t("customTheme.deleteInUseBody") : t("customTheme.deleteBody")
        }
        confirmLabel={t("customTheme.delete")}
        destructive
        busy={busy}
        onConfirm={() => void remove()}
        onCancel={() => setDeleteAsk(false)}
      />
    </>
  );
}

function ListLabel({ children }: { children: React.ReactNode }) {
  return (
    <div className="px-2 pt-2 pb-1 text-[10px] font-semibold tracking-wide text-muted-foreground uppercase">
      {children}
    </div>
  );
}

function ThemeRow({
  def,
  name,
  selected,
  inUse,
  onClick,
}: {
  def: ThemeDef;
  name: string;
  selected: boolean;
  inUse: boolean;
  onClick: () => void;
}) {
  const { t } = useTranslation();
  const c = def.colors;
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={selected || undefined}
      className={cn(
        "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-surface-2",
        selected && "bg-surface-2 font-medium ring-1 ring-primary",
      )}
    >
      <span
        aria-hidden
        className="flex h-5 w-8 shrink-0 overflow-hidden rounded border"
        style={{ borderColor: c["--border"] }}
      >
        <span className="flex-1" style={{ background: c["--bg"] }} />
        <span className="flex-1" style={{ background: c["--text"] }} />
        <span className="flex-1" style={{ background: c["--primary"] }} />
      </span>
      <span className="min-w-0 flex-1 truncate">{name}</span>
      {inUse && (
        <span className="shrink-0 rounded-full border border-primary px-1.5 text-[10px] text-primary">
          {t("customTheme.inUseBadge")}
        </span>
      )}
    </button>
  );
}

function ContrastSummary({ issues }: { issues: ReturnType<typeof contrastIssues> }) {
  const { t } = useTranslation();
  if (issues.length === 0) {
    return (
      <p className="flex items-center gap-1.5 text-[11px] text-success">
        <Check className="size-3.5" />
        {t("customTheme.contrastOk")}
      </p>
    );
  }
  const label = (token: string) => t(`customTheme.token.${tokenKey(token as ThemeToken)}`);
  return (
    <div className="space-y-1 rounded-md border border-warning/50 bg-warning/5 p-2" role="status">
      <p className="flex items-center gap-1.5 font-medium text-warning">
        <TriangleAlert className="size-3.5" />
        {t("customTheme.contrastIssues", { count: issues.length })}
      </p>
      <ul className="list-disc space-y-0.5 pl-5 text-[11px]">
        {issues.map((issue) => (
          <li key={`i-${issue.fg}-${issue.bg}`}>
            {t(issue.graphic ? "customTheme.contrastGraphic" : "customTheme.contrastText", {
              fg: label(issue.fg),
              bg: label(issue.bg),
              ratio: formatRatio(issue.ratio),
              min: issue.min.toFixed(1),
            })}
          </li>
        ))}
      </ul>
    </div>
  );
}

function ColorField({
  token,
  value,
  readOnly,
  warn,
  onChange,
}: {
  token: ThemeToken;
  value: string;
  readOnly: boolean;
  warn: boolean;
  onChange: (value: string) => void;
}) {
  const { t } = useTranslation();
  const [text, setText] = useState(value);
  const [shown, setShown] = useState(value);
  if (shown !== value) {
    setShown(value);
    setText(value);
  }
  const label = t(`customTheme.token.${tokenKey(token)}`);
  const valid = normalizeHex(text) !== null;
  return (
    <div className="flex items-center gap-2 rounded-md border p-1.5">
      <input
        type="color"
        value={value}
        disabled={readOnly}
        aria-label={label}
        onChange={(e) => onChange(e.target.value.toLowerCase())}
        className="h-7 w-9 shrink-0 cursor-pointer rounded border bg-transparent p-0.5 disabled:cursor-default"
      />
      <span className="min-w-0 flex-1 leading-tight">
        <span className="flex items-center gap-1 truncate">
          {label}
          {warn && (
            <TriangleAlert
              className="size-3 shrink-0 text-warning"
              aria-label={t("customTheme.lowContrast")}
            />
          )}
        </span>
        <span className="block font-mono text-[10px] text-muted-foreground">{token}</span>
      </span>
      <input
        value={text}
        readOnly={readOnly}
        aria-label={t("customTheme.hexOf", { name: label })}
        aria-invalid={!valid || undefined}
        spellCheck={false}
        maxLength={7}
        onChange={(e) => {
          setText(e.target.value);
          const hex = normalizeHex(e.target.value);
          if (hex && e.target.value.trim().replace("#", "").length === 6) onChange(hex);
        }}
        onBlur={() => {
          const hex = normalizeHex(text);
          if (hex) onChange(hex);
          setText(hex ?? value);
        }}
        className="h-7 w-[4.75rem] shrink-0 rounded border bg-background px-1.5 font-mono text-[11px] focus:border-primary focus:outline-none aria-invalid:border-destructive"
      />
    </div>
  );
}
