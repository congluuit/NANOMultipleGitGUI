import process from "node:process";
import { defineConfig, devices } from "@playwright/test";

const PORT = 1439;
const CI = !!process.env.CI;

/**
 * End-to-end tests of the interface, run in a real browser against the web build with the
 * fake backend (`?mock=shell`, see `src/features/shell/dev/mockShell.ts`): no Tauri, no git.
 * The risky git flows themselves are covered by the Rust tests on real repositories.
 */
export default defineConfig({
  testDir: "./e2e",
  // Not `*.test.ts` / `*.spec.ts`: those belong to vitest.
  testMatch: "**/*.e2e.ts",
  timeout: 30_000,
  fullyParallel: true,
  forbidOnly: CI,
  retries: CI ? 1 : 0,
  reporter: CI ? "github" : "list",
  outputDir: "./node_modules/.tmp/playwright",
  use: {
    baseURL: `http://localhost:${PORT}`,
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"], viewport: { width: 1400, height: 850 } },
    },
  ],
  webServer: {
    command: `node node_modules/vite/bin/vite.js --port ${PORT} --strictPort`,
    url: `http://localhost:${PORT}`,
    reuseExistingServer: !CI,
    timeout: 120_000,
  },
});
