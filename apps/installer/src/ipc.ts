import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { SetupInfo, Stage } from "./model";

/** Sample data so `pnpm dev` in a plain browser shows every page. */
const browserPreview: SetupInfo = {
  version: "0.0.0-dev",
  defaultDir: "C:\\Users\\you\\AppData\\Local\\NANOMultipleGitGUI",
  existing: null,
  hasPayload: false,
  appRunning: false,
  license: "Apache License\nVersion 2.0, January 2004\n\n(preview)",
};

export const setupInfo = (): Promise<SetupInfo> =>
  isTauri() ? invoke<SetupInfo>("setup_info") : Promise.resolve(browserPreview);

export const appRunning = (): Promise<boolean> =>
  isTauri() ? invoke<boolean>("app_running") : Promise.resolve(false);

export const checkFolder = (dir: string): Promise<void> =>
  isTauri() ? invoke("check_folder", { dir }) : Promise.resolve();

export const pickFolder = (current: string): Promise<string | null> =>
  isTauri() ? invoke<string | null>("pick_folder", { current }) : Promise.resolve(null);

export const install = (dir: string, desktopShortcut: boolean): Promise<void> =>
  isTauri() ? invoke("install", { dir, desktopShortcut }) : Promise.reject(new Error("noPayload"));

export const launchAndQuit = (dir: string): Promise<void> => invoke("launch_and_quit", { dir });

export const quit = (): Promise<void> => (isTauri() ? invoke("quit") : Promise.resolve());

export const minimize = (): Promise<void> =>
  isTauri() ? getCurrentWindow().minimize() : Promise.resolve();

export function onStage(handler: (stage: Stage) => void): () => void {
  if (!isTauri()) return () => {};
  const unlisten = listen<Stage>("setup-stage", (e) => handler(e.payload));
  return () => void unlisten.then((f) => f());
}

/** Rejections from Rust commands are the error code string. */
export const errorCode = (e: unknown): string =>
  typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
