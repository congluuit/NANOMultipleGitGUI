import { defineConfig } from "@playwright/test";

/**
 * Tests that drive the real desktop app (see `e2e-real/app.real.ts`). Not part of CI: they
 * need Windows, a debug build of the app and a throw-away profile.
 */
export default defineConfig({
  testDir: "./e2e-real",
  testMatch: "**/*.real.ts",
  timeout: 120_000,
  workers: 1,
  reporter: "list",
  outputDir: "./node_modules/.tmp/playwright-real",
  webServer: {
    // The debug build loads its interface from the dev server (tauri.conf.json devUrl).
    command: "node node_modules/vite/bin/vite.js --port 1420 --strictPort",
    url: "http://localhost:1420",
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
