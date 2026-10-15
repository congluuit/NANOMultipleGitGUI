import { useEffect } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useSettings } from "@/stores/settings";

/**
 * Applies Settings → "Text size". Webview zoom (like Ctrl + in a browser) scales px values,
 * canvases and viewport units together; CSS `zoom` is only the fallback outside Tauri.
 */
export function useUiScale() {
  const scale = useSettings((s) => s.uiScale);
  useEffect(() => {
    if (isTauri()) {
      getCurrentWebview()
        .setZoom(scale)
        .catch((e: unknown) => console.error("Failed to set zoom:", e));
    } else {
      document.documentElement.style.zoom = scale === 1 ? "" : String(scale);
    }
  }, [scale]);
}
