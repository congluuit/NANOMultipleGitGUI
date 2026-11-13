import { expect, test, type Page } from "@playwright/test";

const open = async (page: Page, flags = "") => {
  await page.goto(`/?mock=shell&lang=en${flags}`);
  await expect(page.getByRole("toolbar")).toBeVisible();
};

/** Runs a command through the palette, the way a user would. */
const runCommand = async (page: Page, query: string, title: string | RegExp) => {
  await page.keyboard.press("Control+k");
  const palette = page.getByRole("dialog");
  await palette.getByRole("combobox").or(palette.getByRole("textbox")).first().fill(query);
  await palette.getByText(title).first().click();
};

test("the shell shows the open repositories", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("tab", { name: /backend-api/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.getByRole("tab")).toHaveCount(3);
  await expect(page.getByText("Recent", { exact: true })).toBeVisible();
});

test("all repositories: overview, then jump to one", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "All repos" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "All repositories" })).toBeVisible();
  await expect(dialog.getByRole("row")).toHaveCount(4);
  await expect(dialog.getByRole("button", { name: "Pull all (1)" })).toBeEnabled();
  await expect(dialog.getByText("12 changed files")).toBeVisible();

  await dialog.getByRole("row", { name: /game-assets/ }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByRole("tab", { name: /game-assets/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
});

test("bisect: answer until the first broken commit is named", async ({ page }) => {
  await open(page, "&bisect");
  await expect(page.getByText("Finding the broken commit.")).toBeVisible();
  const broken = page.getByRole("button", { name: "It is broken" });
  const found = page.getByText("First broken commit:");
  // Each answer halves the candidates; the button goes away once the commit is named.
  for (let i = 0; i < 8 && !(await found.isVisible()); i += 1) {
    await broken.click({ timeout: 2_000 }).catch(() => undefined);
    await expect(broken.or(found).first()).toBeEnabled();
  }
  await expect(found).toBeVisible();
  await page.getByRole("button", { name: "Finish" }).click();
  await expect(page.getByText("First broken commit:")).toBeHidden();
});

test("AI review asks before anything is sent", async ({ page }) => {
  await open(page, "&ai");
  await page.getByTestId("ai-review").click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText(/never runs by itself/)).toBeVisible();
  await expect(dialog.getByTestId("ai-review-text")).toHaveCount(0);

  await dialog.getByRole("button", { name: "Review my changes" }).click();
  await expect(dialog.getByTestId("ai-review-text")).toContainText("src/auth/token.ts");
  await dialog.getByRole("button", { name: "Close" }).first().click();
  await expect(dialog).toBeHidden();
});

test("no AI button without AI set up", async ({ page }) => {
  await open(page);
  await expect(page.getByTestId("ai-review")).toHaveCount(0);
});

test("reflog: filter the list of where HEAD has been", async ({ page }) => {
  await open(page);
  await runCommand(page, "reflog", /Find a lost commit/);
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "Where HEAD has been" })).toBeVisible();
  await expect(dialog.getByRole("listitem")).toHaveCount(7);
  await dialog.getByLabel(/Filter by action/).fill("amend");
  await expect(dialog.getByRole("listitem")).toHaveCount(1);
  await expect(dialog.getByText("Fix token refresh race").first()).toBeVisible();
});

test("sparse checkout: keep four of five folders", async ({ page }) => {
  await open(page);
  await runCommand(page, "sparse", /Sparse checkout/);
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("checkbox", { name: "assets" }).uncheck();
  await dialog.getByRole("button", { name: "Keep 4 folders" }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByText("Sparse checkout: 4 folders kept on disk.")).toBeVisible();
});

test("statistics show authors and the most changed files", async ({ page }) => {
  await open(page);
  await runCommand(page, "statistics", /Repository statistics/);
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("1,284")).toBeVisible();
  await expect(dialog.getByText("Hoang Nguyen")).toBeVisible();
  await expect(dialog.getByText("session.ts")).toBeVisible();
});
