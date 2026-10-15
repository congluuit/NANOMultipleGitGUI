import { useEffect, useMemo, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { findTheme } from "@/features/custom-theme/model";
import { useCustomThemes } from "@/features/custom-theme/store";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";
import { applyCustomColors, readCustomCache, writeCustomCache } from "./custom";
import { applyTheme, resolveTheme, systemPrefersDark, watchSystemTheme } from "./theme";

/** Keeps `<html data-theme>` and the native window theme in sync with the user's choice. */
export function useThemeSync() {
  const mode = useSettings((s) => s.theme);
  const [systemDark, setSystemDark] = useState(systemPrefersDark);
  // Phase 6.2: custom themes (preview while the editor is open, then the active one).
  const themes = useCustomThemes((s) => s.themes);
  const activeId = useCustomThemes((s) => s.activeId);
  const loaded = useCustomThemes((s) => s.loaded);
  const preview = useCustomThemes((s) => s.preview);
  const [startupCache] = useState(readCustomCache);

  useEffect(() => watchSystemTheme(setSystemDark), []);

  useEffect(() => {
    void useCustomThemes.getState().load();
  }, []);

  const active = useMemo(
    () => (mode === "custom" ? findTheme(activeId, themes) : null),
    [mode, activeId, themes],
  );
  // Until the stored themes are read, keep what the first paint used.
  const custom =
    preview ??
    (active
      ? { base: active.base, colors: active.colors }
      : mode === "custom" && !loaded
        ? startupCache
        : null);

  useEffect(() => {
    if (mode === "custom" && loaded && !active) useSettings.getState().setTheme("system");
  }, [mode, loaded, active]);

  useEffect(() => {
    if (preview) return;
    if (active) writeCustomCache({ base: active.base, colors: active.colors });
    else if (mode !== "custom") writeCustomCache(null);
  }, [mode, active, preview]);

  const followsSystem = !custom && (mode === "system" || mode === "custom");
  useEffect(() => {
    if (!isTauri() || !followsSystem) return;
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void win.theme().then((t) => !disposed && t && setSystemDark(t === "dark"));
    void win
      .onThemeChanged(({ payload }) => setSystemDark(payload === "dark"))
      .then((fn) => (disposed ? fn() : (unlisten = fn)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [followsSystem]);

  const resolved = resolveTheme(mode, systemDark, custom?.base);
  // The native title bar follows the base of a custom theme.
  const nativeTheme = custom ? custom.base : followsSystem ? null : resolved;
  useEffect(() => {
    if (!isTauri()) return;
    void getCurrentWindow()
      .setTheme(nativeTheme)
      .catch(() => {});
  }, [nativeTheme]);

  const colorsKey = custom ? JSON.stringify(custom.colors) : "";
  useEffect(() => {
    applyTheme(resolved);
    applyCustomColors(colorsKey ? (JSON.parse(colorsKey) as Record<string, string>) : null);
    // Renderers that read tokens (Monaco, xterm, graph canvas) repaint on `themeVersion`.
    useUi.setState((s) => ({ resolvedTheme: resolved, themeVersion: s.themeVersion + 1 }));
  }, [resolved, colorsKey]);
  return resolved;
}
