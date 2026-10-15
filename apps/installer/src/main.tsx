import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { detectLanguage } from "./i18n";
import "./index.css";

const dark = window.matchMedia("(prefers-color-scheme: dark)");
const applyTheme = () => {
  document.documentElement.dataset.theme = dark.matches ? "dark" : "light";
};
applyTheme();
dark.addEventListener("change", applyTheme);

// The setup is a single fixed window: no browser context menu or reload shortcuts.
if (!import.meta.env.DEV) {
  window.addEventListener("contextmenu", (e) => e.preventDefault());
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App initialLanguage={detectLanguage(navigator.language)} />
  </StrictMode>,
);
