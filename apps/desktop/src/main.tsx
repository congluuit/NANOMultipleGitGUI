import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import App from "./App";
import { openRepository } from "./features/repos/actions";
import { startSettingsSync } from "./features/settings/settingsSync";
import { initI18n } from "./i18n";
import { commands } from "./ipc";
import { queryClient } from "./lib/queryClient";
import { useSettings } from "./stores/settings";
import { useUi } from "./stores/ui";
import { useWorkspace } from "./stores/workspace";
import { applyTheme, resolveTheme, systemPrefersDark } from "./theme/theme";
import "./index.css";

if (import.meta.env.DEV) {
  // Dev-only overrides for screenshots, e.g. http://localhost:1420/?theme=dark&lang=en&view=palette
  const params = new URLSearchParams(location.search);
  const theme = params.get("theme");
  const lang = params.get("lang");
  if (theme === "light" || theme === "dark" || theme === "system") {
    useSettings.getState().setTheme(theme);
  }
  if (lang === "vi" || lang === "en") useSettings.getState().setLanguage(lang);
  if (params.get("view") === "palette") useUi.getState().setView("palette");
}

const { theme, language } = useSettings.getState();
applyTheme(resolveTheme(theme, systemPrefersDark()));
initI18n(language);
void startSettingsSync();

// Restore the saved tabs, then open a repository passed on the command line (`nanomgg <path>`).
void useWorkspace
  .getState()
  .load()
  .then(async () => {
    if (!isTauri()) return;
    const { repo, benchGraph } = await commands.startupArgs();
    if (repo) {
      useUi.setState({ benchOnLoad: benchGraph });
      await openRepository(repo);
    }
  })
  .catch((e: unknown) => console.error("Failed to restore workspace", e));

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root element");

createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
);
