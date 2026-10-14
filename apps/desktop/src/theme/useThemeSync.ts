import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";
import { applyTheme, resolveTheme, systemPrefersDark, watchSystemTheme } from "./theme";

/** Keeps `<html data-theme>` and the native window theme in sync with the user's choice. */
export function useThemeSync() {
  const mode = useSettings((s) => s.theme);
  const [systemDark, setSystemDark] = useState(systemPrefersDark);

  useEffect(() => watchSystemTheme(setSystemDark), []);

  useEffect(() => {
    if (!isTauri() || mode !== "system") return;
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
  }, [mode]);

  useEffect(() => {
    if (!isTauri()) return;
    void getCurrentWindow()
      .setTheme(mode === "system" ? null : mode)
      .catch(() => {});
  }, [mode]);

  const resolved = resolveTheme(mode, systemDark);
  useEffect(() => {
    applyTheme(resolved);
    useUi.setState({ resolvedTheme: resolved });
  }, [resolved]);
  return resolved;
}
