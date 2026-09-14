import { expect, test, type Page } from "@playwright/test";
import axe from "axe-core";

const evidenceDirectory = "/tmp/foldry-playwright-qa";
const desktopViewports = [
  { width: 1024, height: 700 },
  { width: 1280, height: 800 },
  { width: 1440, height: 900 },
] as const;

type AccessibilityResult = {
  violations: Array<{
    id: string;
    impact: string | null;
    nodes: Array<{ target: string[] }>;
  }>;
};

async function openFolders(page: Page) {
  await page.goto("/");
  await expect(page).toHaveTitle("Foldry");
  await expect(
    page.getByRole("heading", { name: "Folders", level: 1 }),
  ).toBeVisible();
  await expect(page.locator("#root")).not.toBeEmpty();
  await expect(page.locator("vite-error-overlay")).toHaveCount(0);
}

async function expectNoHorizontalOverflow(page: Page) {
  const overflow = await page.evaluate(() => ({
    body: document.body.scrollWidth - document.body.clientWidth,
    document:
      document.documentElement.scrollWidth -
      document.documentElement.clientWidth,
  }));
  expect(overflow).toEqual({ body: 0, document: 0 });
}

async function expectNoAccessibilityViolations(page: Page) {
  await page.addScriptTag({ content: axe.source });
  const result = await page.evaluate<AccessibilityResult>(async () => {
    const runner = (
      globalThis as unknown as {
        axe: {
          run: (
            root: Document,
            options: { runOnly: { type: "tag"; values: string[] } },
          ) => Promise<AccessibilityResult>;
        };
      }
    ).axe;
    return runner.run(document, {
      runOnly: {
        type: "tag",
        values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"],
      },
    });
  });
  expect(
    result.violations,
    result.violations
      .map(
        (violation) =>
          `${violation.id} (${violation.impact ?? "unknown"}): ${violation.nodes
            .flatMap((node) => node.target)
            .join(", ")}`,
      )
      .join("\n"),
  ).toEqual([]);
}

test("captures the light and dark desktop matrix without overflow", async ({
  page,
}) => {
  const consoleProblems: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error" || message.type() === "warning") {
      consoleProblems.push(`${message.type()}: ${message.text()}`);
    }
  });
  page.on("pageerror", (error) => consoleProblems.push(`pageerror: ${error}`));

  await openFolders(page);
  await expect(page.locator("html")).toHaveAttribute(
    "data-mantine-color-scheme",
    "light",
  );

  for (const viewport of desktopViewports) {
    await page.setViewportSize(viewport);
    await expectNoHorizontalOverflow(page);
    await page.screenshot({
      path: `${evidenceDirectory}/light-${viewport.width}x${viewport.height}.png`,
    });
  }

  await page.getByRole("button", { name: "Switch to dark theme" }).click();
  await expect(page.locator("html")).toHaveAttribute(
    "data-mantine-color-scheme",
    "dark",
  );

  for (const viewport of desktopViewports) {
    await page.setViewportSize(viewport);
    await expectNoHorizontalOverflow(page);
    await page.screenshot({
      path: `${evidenceDirectory}/dark-${viewport.width}x${viewport.height}.png`,
    });
  }

  await page.setViewportSize({ width: 1280, height: 800 });
  await page.getByRole("button", { name: "Ignore Profiles" }).click();
  await expect(
    page.getByRole("heading", { name: "Ignore Profiles", level: 2 }),
  ).toBeVisible();
  await expectNoHorizontalOverflow(page);
  await expectNoAccessibilityViolations(page);
  await page.screenshot({
    path: `${evidenceDirectory}/dark-profiles-1280x800.png`,
  });

  const presetsHeading = page.getByRole("heading", {
    name: "Ignore Presets",
    level: 2,
  });
  for (const width of [1024, 800, 640, 375, 320]) {
    await page.setViewportSize({ width, height: width < 700 ? 900 : 800 });
    await expect(presetsHeading).toBeVisible();
    await expectNoHorizontalOverflow(page);
  }
  await presetsHeading.scrollIntoViewIfNeeded();
  await page.screenshot({
    path: `${evidenceDirectory}/dark-profiles-presets-320x900.png`,
  });

  expect(consoleProblems).toEqual([]);
});

test("validates search, overflow menu, keyboard focus, and reduced motion", async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openFolders(page);
  expect(
    await page.evaluate(
      () => matchMedia("(prefers-reduced-motion: reduce)").matches,
    ),
  ).toBe(true);

  const cards = page.getByRole("article");
  await expect(cards).toHaveCount(2);
  const firstCard = cards.first();
  const moreActions = firstCard.getByRole("button", { name: "More actions" });
  await expect(
    firstCard.getByRole("button", { name: "Run enabled actions" }),
  ).toBeVisible();
  await expect(
    firstCard.getByRole("button", { name: "Remove from Folders" }),
  ).toBeVisible();
  await expect(moreActions).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "Preview" })).toHaveCount(0);

  await moreActions.hover();
  await expect(page.getByRole("menuitem", { name: "Preview" })).toHaveCount(0);
  await moreActions.click();
  await expect(page.getByRole("menuitem", { name: "Preview" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menuitem", { name: "Preview" })).toHaveCount(0);
  await moreActions.focus();
  await expect(moreActions).toBeFocused();
  expect(
    await moreActions.evaluate((element) => {
      const style = getComputedStyle(element);
      return (
        (style.outlineStyle !== "none" && parseFloat(style.outlineWidth) > 0) ||
        style.boxShadow !== "none"
      );
    }),
  ).toBe(true);
  await page.screenshot({
    path: `${evidenceDirectory}/overflow-keyboard-focus-1280x720.png`,
  });
  await moreActions.press("Enter");
  await expect(page.getByRole("menuitem", { name: "Preview" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menuitem", { name: "Preview" })).toHaveCount(0);
  await moreActions.press("Space");
  await expect(page.getByRole("menuitem", { name: "Preview" })).toBeVisible();
  await page.keyboard.press("Escape");

  const search = page.getByRole("textbox", { name: "Search folders" });
  await search.fill("documents");
  await expect(cards).toHaveCount(1);
  await expect(cards.first()).toContainText(
    "C:\\Users\\Alice\\Documents\\Work",
  );
  await search.fill("work");
  await expect(cards).toHaveCount(2);
  await search.fill("does-not-exist");
  await expect(page.getByText("No folders match this search.")).toBeVisible();
  await search.clear();
  await expect(cards).toHaveCount(2);

  await expectNoAccessibilityViolations(page);
});

test("opens, exercises, and closes the Activity drawer", async ({ page }) => {
  const consoleProblems: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error" || message.type() === "warning") {
      consoleProblems.push(`${message.type()}: ${message.text()}`);
    }
  });
  page.on("pageerror", (error) => consoleProblems.push(`pageerror: ${error}`));

  await openFolders(page);
  const activityTrigger = page.getByRole("button", { name: /Activity · 1\/1/ });
  await activityTrigger.focus();
  await expect(activityTrigger).toBeFocused();
  await activityTrigger.press("Enter");

  const drawer = page.getByRole("dialog", { name: "Activity" });
  await expect(drawer).toBeVisible();
  await expect(drawer.getByRole("heading", { name: "Active" })).toBeVisible();
  await expect(drawer.getByRole("heading", { name: "Waiting" })).toBeVisible();
  await expect(
    drawer.getByRole("button", { name: "Recheck changed" }),
  ).toBeVisible();
  await expect(drawer.getByText(/archive · running/)).toBeVisible();
  await expect(drawer.getByText(/archive · Queue position 1/)).toBeVisible();

  await drawer.getByRole("button", { name: "Recheck changed" }).click();
  await expect(
    drawer.getByRole("button", { name: "Recheck changed" }),
  ).toBeEnabled();
  await page.screenshot({
    path: `${evidenceDirectory}/activity-drawer-1280x720.png`,
  });
  await expectNoAccessibilityViolations(page);

  await page.keyboard.press("Escape");
  await expect(drawer).toBeHidden();
  await expect(activityTrigger).toBeFocused();
  expect(consoleProblems).toEqual([]);
});
