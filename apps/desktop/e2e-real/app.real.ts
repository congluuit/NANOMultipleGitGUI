/**
 * Drives the real desktop app (debug build, real Rust backend, real git) through its
 * WebView2 debugging port, on a throw-away profile and a throw-away repository.
 *
 * Needs Windows, `cargo build -p nanomgg` and the folders made by `e2e-real/setup.ps1`:
 *
 *   pnpm --filter @nanomgg/desktop test:e2e:real
 *
 * `NANOMGG_REAL_GITEA_URL` / `NANOMGG_REAL_GITEA_TOKEN` (optional) also sign in to a real
 * Gitea / Forgejo server through the "Add account" wizard.
 * `NANOMGG_REAL_OLLAMA_URL` / `NANOMGG_REAL_OLLAMA_MODEL` (optional) run the AI review with a
 * real model.
 */
import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { chromium, expect, test, type Browser, type Page } from "@playwright/test";

const ROOT = process.env.NANOMGG_REAL_ROOT ?? "E:\\AITempKnowledge\\nanomgg-real-e2e";
const EXE =
  process.env.NANOMGG_REAL_EXE ?? path.resolve(process.cwd(), "../../target/debug/nanomgg.exe");
const REPO = path.join(ROOT, "repo");
const CDP_PORT = 9333;

const git = (...args: string[]) =>
  execFileSync("git", args, { cwd: REPO, encoding: "utf8" }).trim();

let app: ChildProcess;
let browser: Browser;
let page: Page;

test.beforeAll(async () => {
  test.skip(process.platform !== "win32", "drives WebView2: Windows only");
  expect(existsSync(EXE), `build the app first: ${EXE}`).toBe(true);
  expect(existsSync(path.join(REPO, ".git")), "run e2e-real/setup.ps1 first").toBe(true);
  app = spawn(EXE, ["--open", REPO], {
    env: {
      ...process.env,
      // Logs, database and settings of this run stay out of the user's profile.
      NANOMGG_PROFILE_DIR: path.join(ROOT, "profile"),
      WEBVIEW2_USER_DATA_FOLDER: path.join(ROOT, "webview"),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP_PORT}`,
    },
    stdio: "ignore",
  });
  await expect
    .poll(
      async () => {
        try {
          browser = await chromium.connectOverCDP(`http://127.0.0.1:${CDP_PORT}`);
          return true;
        } catch {
          return false;
        }
      },
      { timeout: 60_000, message: "the app's webview did not open its debugging port" },
    )
    .toBe(true);
  page = browser.contexts()[0]!.pages()[0]!;
  await page.waitForLoadState("domcontentloaded");
  // A new profile starts with the first-run setup (and may offer to import accounts).
  const toolbar = page.getByRole("toolbar");
  const skip = page.getByRole("button", { name: /^Skip/ });
  await expect(toolbar.or(skip).first()).toBeVisible({ timeout: 30_000 });
  for (let i = 0; i < 3 && (await skip.first().isVisible()); i += 1) {
    await skip.first().click();
    await page.waitForTimeout(500);
  }
  await expect(toolbar).toBeVisible({ timeout: 30_000 });
});

test.afterAll(async () => {
  await browser?.close().catch(() => undefined);
  // The whole tree: the webview processes hold the profile folder open.
  if (app?.pid) {
    try {
      execFileSync("taskkill", ["/pid", String(app.pid), "/T", "/F"], { stdio: "ignore" });
    } catch {
      app.kill();
    }
  }
});

/** Runs a command through the palette, the way a user would. */
async function runCommand(query: string, title: string | RegExp) {
  await page.keyboard.press("Control+k");
  const palette = page.getByRole("dialog");
  await palette.getByRole("combobox").or(palette.getByRole("textbox")).first().fill(query);
  await palette.getByText(title).first().click();
}

const dialog = () => page.getByRole("dialog");

interface AppWindow {
  __TAURI_INTERNALS__: {
    invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
  };
  __nanomggOpenTool: (tool: Record<string, unknown>) => void;
}

/** Calls a command of the real backend from inside the app's page. */
const invoke = <T>(cmd: string, args: Record<string, unknown> = {}) =>
  page.evaluate(
    ([c, a]) =>
      (globalThis as unknown as AppWindow).__TAURI_INTERNALS__.invoke(
        c as string,
        a as Record<string, unknown>,
      ),
    [cmd, args] as const,
  ) as Promise<T>;

test.describe.configure({ mode: "serial" });

test("opens the repository and lists recent branches from git's reflog", async () => {
  await expect(page.getByRole("tab", { name: /repo/ })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByText("Recent", { exact: true })).toBeVisible();
  // `feature/discount` was only ever checked out with the git CLI, before the app ran.
  await expect(page.getByText("feature/discount").first()).toBeVisible();
  // AI is off on a new profile: no AI button anywhere near the commit box.
  await expect(page.getByTestId("ai-review")).toHaveCount(0);
  await expect(page.getByTestId("ai-suggest")).toHaveCount(0);
});

test("highlights the changed words inside an edited line", async () => {
  await page.getByText("calc.js").first().click();
  const marks = page.locator("mark[data-word-change]");
  await expect(marks.first()).toBeVisible({ timeout: 15_000 });
  const words = await marks.allInnerTexts();
  expect(words).toContain("count");
  expect(words).toContain("quantity");
  await page.keyboard.press("Escape");
});

test("statistics come from the real history", async () => {
  await runCommand("statistics", /Repository statistics/);
  await expect(dialog().getByRole("heading", { name: "Repository statistics" })).toBeVisible();
  await expect(dialog().getByText("Tester")).toBeVisible();
  await expect(dialog().getByText("guide.md")).toBeVisible();
  const commits = Number(git("rev-list", "--count", "HEAD"));
  await expect(dialog().getByText(String(commits), { exact: true }).first()).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog()).toBeHidden();
});

test("the reflog lists where HEAD has been", async () => {
  await runCommand("reflog", /Find a lost commit/);
  await expect(dialog().getByRole("heading", { name: "Where HEAD has been" })).toBeVisible();
  const total = git("reflog").split("\n").length;
  await expect(dialog().getByRole("listitem")).toHaveCount(total);
  await dialog()
    .getByLabel(/Filter by action/)
    .fill("moving from");
  await expect(dialog().getByRole("listitem").first()).toContainText("checkout");
  await page.keyboard.press("Escape");
  await expect(dialog()).toBeHidden();
});

test("all repositories: fetch all reports the round", async () => {
  await runCommand("all repositories", /All repositories/);
  await expect(dialog().getByRole("heading", { name: "All repositories" })).toBeVisible();
  await expect(dialog().getByRole("row", { name: /repo/ }).last()).toContainText("main");
  await expect(dialog().getByText("1 changed file")).toBeVisible();
  await dialog().getByRole("button", { name: "Fetch all" }).click();
  await expect(page.getByText("Fetched 1 repository.")).toBeVisible({ timeout: 30_000 });
  await page.keyboard.press("Escape");
});

test("AI review: off until set up, asks first, then a real model answers", async () => {
  const url = process.env.NANOMGG_REAL_OLLAMA_URL;
  const model = process.env.NANOMGG_REAL_OLLAMA_MODEL;
  test.skip(!url || !model, "set NANOMGG_REAL_OLLAMA_URL and NANOMGG_REAL_OLLAMA_MODEL");
  test.setTimeout(300_000);
  // The user turns AI on (here through the same command the settings window uses).
  const { config } = await invoke<{ config: Record<string, unknown> & { models: object } }>(
    "ai_settings",
  );
  const withAi = (enabled: boolean) => ({
    config: {
      ...config,
      enabled,
      provider: "ollama",
      ollamaUrl: url,
      models: { ...config.models, ollama: model },
    },
  });
  await invoke("ai_settings_save", withAi(true));
  await page.reload();
  await expect(page.getByRole("toolbar")).toBeVisible({ timeout: 30_000 });
  git("add", "src/calc.js");

  const button = page.getByTestId("ai-review");
  await expect(button).toBeVisible({ timeout: 20_000 });
  await button.click();
  // Nothing is sent yet: the window says what would be sent and waits for a yes.
  await expect(dialog().getByText(/never runs by itself/)).toBeVisible();
  await expect(dialog().getByText(/stay on this computer/)).toBeVisible();
  await expect(dialog().getByTestId("ai-review-text")).toHaveCount(0);

  await dialog().getByRole("button", { name: "Review my changes" }).click();
  const text = dialog().getByTestId("ai-review-text");
  await expect(text).toBeVisible({ timeout: 240_000 });
  const review = await text.innerText();
  console.log(`--- AI review in the real app (${model}) ---\n${review}\n---`);
  expect(review.trim().length).toBeGreaterThan(20);
  await dialog().getByRole("button", { name: "Close" }).first().click();
  await expect(dialog()).toBeHidden();

  git("reset", "-q");
  await invoke("ai_settings_save", withAi(false));
});

test("sparse checkout removes a folder from disk and brings it back", async () => {
  await runCommand("sparse", /Sparse checkout/);
  await dialog().getByRole("checkbox", { name: "assets" }).uncheck();
  await dialog().getByRole("button", { name: "Keep 2 folders" }).click();
  await expect(page.getByText("Sparse checkout: 2 folders kept on disk.")).toBeVisible({
    timeout: 20_000,
  });
  expect(existsSync(path.join(REPO, "assets", "logo.txt"))).toBe(false);
  expect(existsSync(path.join(REPO, "src", "calc.js"))).toBe(true);

  await runCommand("sparse", /Sparse checkout/);
  await expect(dialog().getByText(/Sparse checkout is on/)).toBeVisible();
  await dialog()
    .getByRole("button", { name: /Turn off/ })
    .click();
  await expect(page.getByText("Sparse checkout is off. Every file is back.")).toBeVisible({
    timeout: 20_000,
  });
  expect(existsSync(path.join(REPO, "assets", "logo.txt"))).toBe(true);
});

test("bisect finds the commit that broke the sample", async () => {
  // The sample's bug: "Change 4" turned `price * count` into `price + count`.
  git("checkout", "--", "src/calc.js");
  const good = git("rev-list", "--max-parents=0", "HEAD");
  await runCommand("bisect", /Find the commit that broke it/);
  await dialog().getByLabel("Last commit known to work").fill(good);
  await dialog().getByRole("button", { name: "Start" }).click();
  await expect(page.getByText("Finding the broken commit.")).toBeVisible({ timeout: 20_000 });

  const found = page.getByText("First broken commit:");
  for (let i = 0; i < 8 && !(await found.isVisible()); i += 1) {
    const before = git("rev-parse", "HEAD");
    const broken = readFileSync(path.join(REPO, "src", "calc.js"), "utf8").includes("+");
    await page
      .getByRole("button", { name: broken ? "It is broken" : "It works", exact: true })
      .click();
    await expect
      .poll(async () => (await found.isVisible()) || git("rev-parse", "HEAD") !== before, {
        timeout: 20_000,
      })
      .toBe(true);
  }
  await expect(found).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "First broken commit:" })).toContainText(
    "Change 4",
  );
  await page.getByRole("button", { name: "Finish" }).click();
  await expect(found).toBeHidden({ timeout: 20_000 });
  expect(git("branch", "--show-current")).toBe("main");
});

test("a commit saved as a patch file is applied back as a commit", async () => {
  const sha = git("rev-parse", "HEAD");
  const subject = git("log", "-1", "--format=%s");
  const file = path.join(ROOT, "last-commit.patch");
  const { activeId } = await invoke<{ activeId: number }>("workspace_state");
  const saved = await invoke<{ status: string }>("patch_save_commit", {
    repoId: activeId,
    oid: sha,
    dest: file,
  });
  expect(saved.status).toBe("done");
  expect(readFileSync(file, "utf8")).toContain(`Subject: [PATCH] ${subject}`);

  // Take the commit away, then bring it back from the file through the app's window.
  git("reset", "-q", "--hard", "HEAD~1");
  expect(git("log", "-1", "--format=%s")).not.toBe(subject);
  await page.evaluate(
    ([id, f]) =>
      (globalThis as unknown as AppWindow).__nanomggOpenTool({
        kind: "patchApply",
        repoId: id,
        file: f,
      }),
    [activeId, file] as const,
  );
  await expect(dialog().getByRole("heading", { name: "Apply patch" })).toBeVisible();
  await expect(dialog().getByText("1 commit in this patch")).toBeVisible();
  await expect(dialog().getByText(subject)).toBeVisible();
  await expect(dialog().getByText("guide.md")).toBeVisible();
  await dialog().getByRole("button", { name: "Apply as 1 commit" }).click();
  await expect(page.getByText("Patch applied as commits.")).toBeVisible({ timeout: 20_000 });
  expect(git("log", "-1", "--format=%s")).toBe(subject);
  expect(git("status", "--porcelain")).toBe("");
});

test("settings offer the system notification switch", async () => {
  await runCommand("open settings", /Open settings/);
  const toggle = dialog().getByRole("switch", { name: "System notifications" });
  await expect(toggle).toBeChecked();
  await toggle.click();
  await expect(toggle).not.toBeChecked();
  await toggle.click();
  await expect(toggle).toBeChecked();
  await page.keyboard.press("Escape");
});

test("add account: Bitbucket refuses a wrong token through the real service", async () => {
  await runCommand("add account", /Add account/);
  for (const name of [/Gitea \/ Forgejo/, /Bitbucket/, /Azure DevOps/]) {
    await expect(page.getByRole("radio", { name })).toBeVisible();
  }
  await page.getByRole("radio", { name: /Bitbucket/ }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await page
    .getByRole("list", { name: /sign in/i })
    .getByRole("button")
    .first()
    .click();
  await expect(page.getByText(/Pull requests, issues and CI of this server/)).toBeVisible();
  await page.getByLabel("Email of your Atlassian account").fill("nobody@example.invalid");
  await page.getByLabel("Token").fill("not-a-real-token");
  await page.getByRole("button", { name: "Check and save" }).click();
  // The real bitbucket.org answered 401: the wizard says so and adds nothing.
  await expect(page.getByRole("alert").or(page.getByText(/token/i).last())).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByText("Account added")).toHaveCount(0);
});

test("add account: signs in to a real Gitea server", async () => {
  const url = process.env.NANOMGG_REAL_GITEA_URL;
  const token = process.env.NANOMGG_REAL_GITEA_TOKEN;
  test.skip(!url || !token, "set NANOMGG_REAL_GITEA_URL and NANOMGG_REAL_GITEA_TOKEN");
  // From the token step back to the list of servers.
  const back = page.getByRole("button", { name: "Back", exact: true }).last();
  await back.click();
  await back.click();
  await page.getByRole("radio", { name: /Gitea \/ Forgejo/ }).click();
  await page.getByLabel("Server address").fill(url!);
  await page.getByRole("button", { name: "Continue" }).click();
  await page
    .getByRole("list", { name: /sign in/i })
    .getByRole("button")
    .first()
    .click();
  await page.getByLabel("Token").fill(token!);
  await page.getByRole("button", { name: "Check and save" }).click();
  await expect(page.getByText("Account added")).toBeVisible({ timeout: 30_000 });
});
