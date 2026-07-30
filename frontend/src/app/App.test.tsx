import {
  render,
  screen,
  waitFor,
  waitForElementToBeRemoved,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";

import { App } from "./App";

describe("App shell", () => {
  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.removeAttribute("data-mantine-color-scheme");
  });

  it("restores the typed workspace snapshot", async () => {
    render(<App />);

    expect(
      await screen.findByRole("heading", { name: "Folders" }),
    ).toBeInTheDocument();
    expect(
      screen.getAllByText("C:\\Users\\Alice\\Documents\\Work").length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByText("D:\\Projects\\Work").length).toBeGreaterThan(0);
    expect(
      screen.getByRole("button", { name: "Run all enabled" }),
    ).toBeEnabled();
    expect(screen.queryByText("Open run history")).not.toBeInTheDocument();
    expect(
      screen.getByText("Running progress").parentElement,
    ).not.toHaveTextContent("—");
  });

  it("opens accessible About and Help dialogs from the application header", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Folders" });

    const connected = screen.getByRole("button", { name: "Connected" });
    const aboutButton = screen.getByRole("button", { name: "About" });
    const helpButton = screen.getByRole("button", { name: "Help" });
    const language = screen.getByRole("button", { name: "Language" });

    expect(
      connected.compareDocumentPosition(aboutButton) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).not.toBe(0);
    expect(
      aboutButton.compareDocumentPosition(helpButton) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).not.toBe(0);
    expect(
      helpButton.compareDocumentPosition(language) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).not.toBe(0);

    await user.click(aboutButton);
    const about = await screen.findByRole("dialog", { name: "About Foldry" });
    expect(
      within(about).getByRole("heading", { name: "Foldry" }),
    ).toBeVisible();
    expect(
      within(about).getByText(
        "Foldry helps you prepare folders for safe, repeatable transfer or backup without sending them through a cloud service.",
      ),
    ).toBeVisible();
    expect(within(about).getByLabelText("Version")).toHaveTextContent("v0.1.2");
    expect(
      within(about).getByRole("link", { name: "tychh/foldry" }),
    ).toHaveAttribute("href", "https://github.com/tychh/foldry");
    expect(within(about).getByText("MIT OR Apache-2.0")).toBeVisible();
    expect(within(about).getByText("Rust")).toBeVisible();
    expect(within(about).getByRole("img", { name: "tychh" })).toBeVisible();
    expect(
      within(about).getByText(/Created and maintained by/),
    ).toHaveTextContent("Created and maintained by tychh");
    expect(
      within(about).getByRole("link", {
        name: "If Foldry is useful to you, I’d be grateful for your support.",
      }),
    ).toHaveAttribute("href", "https://ko-fi.com/tychh");

    await user.click(
      within(about).getByRole("button", { name: "Close About Foldry" }),
    );
    await waitForElementToBeRemoved(about);

    await user.click(helpButton);
    const help = await screen.findByRole("dialog", { name: "Foldry Help" });
    expect(
      within(help).getByText(
        "Foldry processes folders individually or in batches, applying Ignore Profiles to keep unnecessary files and directories out of each operation.",
      ),
    ).toBeVisible();
    expect(within(help).getByText("Start with folders")).toBeVisible();
    expect(within(help).getByText("Create verified archives")).toBeVisible();
    expect(
      within(help).getByText(
        "Processing stays on this computer. This release creates local archives; network synchronization is planned for a later version.",
      ),
    ).toBeVisible();
    expect(
      within(help).getByText(
        "Add a folder → choose an Ignore Profile → configure an Archive Action → review Preview → run and inspect the result.",
      ),
    ).toBeVisible();
  });

  it("keeps global pause, stop, and restart states consistent", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Folders" });

    const enabledSwitches = screen.getAllByRole("switch", { name: "Enabled" });
    await user.click(enabledSwitches[1]!);

    expect(screen.getByText("1 running")).toBeInTheDocument();
    expect(screen.getByText("1 queued")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Pause all" }));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Resume all" }),
      ).toBeInTheDocument(),
    );
    expect(screen.getByText("0 running")).toBeInTheDocument();
    expect(screen.getByText("1 paused")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Resume all" }));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Pause all" }),
      ).toBeInTheDocument(),
    );
    expect(screen.getByText("1 running")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Stop all" }));
    await waitFor(() => {
      expect(screen.getByText("0 running")).toBeInTheDocument();
      expect(screen.getByText("0 queued")).toBeInTheDocument();
      expect(screen.getByText("0 paused")).toBeInTheDocument();
    });
    expect(screen.queryByText("Stopping")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Pause all" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Stop all" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Run all enabled" }));
    await waitFor(() =>
      expect(screen.getByText("1 queued")).toBeInTheDocument(),
    );
    expect(screen.queryByText("Stopping")).not.toBeInTheDocument();
  });

  it("routes between folders and the profile workspace", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Folders" });

    await user.click(screen.getByRole("button", { name: "Ignore Profiles" }));

    expect(
      await screen.findByRole(
        "heading",
        { name: "Default", level: 1 },
        { timeout: 5_000 },
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Ignore Profiles", level: 2 }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Ignore Presets", level: 2 }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "New profile" })).toHaveLength(
      1,
    );
    expect(screen.getByText("default.packignore")).toBeInTheDocument();
    expect(screen.getByText("Uses: 2")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Rename profile" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Delete profile" }),
    ).not.toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: "How packignore rules work" }),
    );
    const help = await screen.findByRole("dialog", {
      name: "How .packignore rules work",
    });
    expect(within(help).getByText("**/*.tmp")).toBeVisible();
    expect(
      within(help).getByText(
        "Rules are read from top to bottom. If several rules match, the last one wins.",
      ),
    ).toBeVisible();
    await user.click(within(help).getByRole("button", { name: "Close" }));
    await waitForElementToBeRemoved(help);
  });

  it("keeps CodeMirror focus inside the complete profile editor surface", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Folders" });
    await user.click(screen.getByRole("button", { name: "Ignore Profiles" }));
    await screen.findByRole(
      "heading",
      { name: "Default", level: 1 },
      { timeout: 5_000 },
    );

    const surface = screen.getByTestId("profile-editor-surface");
    expect(surface.querySelector(".foldry-profile-editor")).toBeInTheDocument();
    expect(surface.querySelector(".cm-scroller")).toBeInTheDocument();
    expect(
      document.querySelectorAll(
        '[data-offset-scrollbars="y"][data-scrollbars="y"]',
      ),
    ).toHaveLength(2);
    const editor = surface.querySelector<HTMLElement>(
      '[contenteditable="true"]',
    );
    expect(editor).not.toBeNull();
    await user.click(editor!);

    expect(surface).toContainElement(document.activeElement as HTMLElement);
    expect(surface.querySelector(".cm-editor")).toHaveClass("cm-focused");
  });

  it("creates a valid profile through the typed desktop command", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Folders" });
    await user.click(screen.getByRole("button", { name: "Ignore Profiles" }));

    await user.click(
      await screen.findByRole(
        "button",
        { name: "New profile" },
        { timeout: 5_000 },
      ),
    );
    await user.type(
      await screen.findByLabelText("Profile name"),
      "Release rules",
    );
    await user.click(screen.getByRole("button", { name: "Confirm" }));

    expect(
      await screen.findByRole("heading", { name: "Release rules", level: 1 }),
    ).toBeInTheDocument();
    expect(screen.getByText("release-rules.packignore")).toBeInTheDocument();
  });

  it("inserts sensitive presets directly and confirms their removal", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Folders" });
    await user.click(screen.getByRole("button", { name: "Ignore Profiles" }));

    const presetName = await screen.findByText("Environment and secrets");
    const presetCard = presetName.closest(".mantine-Paper-root");
    expect(presetCard).not.toBeNull();

    await user.click(
      within(presetCard as HTMLElement).getByRole("button", {
        name: "Insert preset",
      }),
    );

    expect(
      screen.queryByText("Remove sensitive preset?"),
    ).not.toBeInTheDocument();
    await user.click(
      within(presetCard as HTMLElement).getByRole("button", {
        name: "Remove preset",
      }),
    );
    expect(
      within(presetCard as HTMLElement).getByText("Remove sensitive preset?"),
    ).toBeInTheDocument();
  });

  it("switches the interface language without hardcoded component copy", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Folders" });

    await user.click(screen.getByRole("button", { name: "Language" }));
    await user.click(await screen.findByText("Русский"));

    expect(screen.getByRole("heading", { name: "Папки" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Запустить все включённые" }),
    ).toBeInTheDocument();
  });

  it("offers an accessible light and dark theme control", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Folders" });
    await user.click(screen.getByRole("button", { name: "Ignore Profiles" }));
    await screen.findByRole(
      "heading",
      { name: "Default", level: 1 },
      { timeout: 5_000 },
    );

    const appIcon = document.querySelector<HTMLImageElement>(
      'img[src="/app-icon-light.png"]',
    );
    const editor = document.querySelector(".cm-editor");
    const editorWrapper = document.querySelector(".foldry-profile-editor");
    expect(appIcon).toBeInTheDocument();
    expect(editor).toBeInTheDocument();
    expect(editorWrapper).toHaveClass("cm-theme-none");

    const toggle = screen.getByRole("button", {
      name: "Switch to dark theme",
    });
    await user.click(toggle);

    expect(
      screen.getByRole("button", { name: "Switch to light theme" }),
    ).toBeInTheDocument();
    expect(appIcon).toHaveAttribute("src", "/app-icon-dark.png");

    await user.click(
      screen.getByRole("button", { name: "Switch to light theme" }),
    );

    expect(
      screen.getByRole("button", { name: "Switch to dark theme" }),
    ).toBeInTheDocument();
    expect(appIcon).toHaveAttribute("src", "/app-icon-light.png");
    expect(document.querySelector(".cm-editor")).toBe(editor);
    expect(editorWrapper).toHaveClass("cm-theme-none");
  });

  it("selects a folder card and exposes its independent action", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Folders" });

    const folderCard = screen.getByRole("button", {
      name: /Work.*D:\\Projects\\Work/,
    });
    await user.click(folderCard);
    const folderHeading = await screen.findByRole("heading", {
      level: 2,
      name: "Work",
    });
    const switches = screen.getAllByRole("switch", { name: "Enabled" });
    const actionNumber = screen.getByText("01");
    const removeAction = screen.getByRole("button", {
      name: "Remove action",
    });
    const archiveHelpButton = screen.getByRole("button", {
      name: "About Archive action",
    });

    expect(
      folderHeading.parentElement?.querySelector('[data-enabled="true"]'),
    ).toBeInTheDocument();
    expect(
      folderCard.querySelector('[data-enabled="true"]'),
    ).toBeInTheDocument();
    expect(switches).toHaveLength(2);
    expect(
      within(folderHeading.parentElement!).getByRole("switch", {
        name: "Enabled",
      }),
    ).toBe(switches[0]);
    expect(
      within(actionNumber.parentElement!).getByRole("switch", {
        name: "Enabled",
      }),
    ).toBe(switches[1]);
    expect(
      within(actionNumber.parentElement!).getByText("Queued"),
    ).toBeInTheDocument();
    expect(screen.queryByText("Enabled")).not.toBeInTheDocument();
    expect(
      screen
        .getByRole("combobox", { name: "Ignore Profile" })
        .closest(".mantine-Select-root"),
    ).toHaveStyle({ width: "100%" });
    expect(actionNumber).toHaveAttribute("data-enabled", "false");
    expect(actionNumber.parentElement).toHaveTextContent("01Archive");
    expect(removeAction).toHaveAttribute("data-variant", "light");
    expect(removeAction).toHaveStyle({ "--ai-size": "1.75rem" });
    expect(archiveHelpButton).toHaveStyle({ "--ai-size": "1.75rem" });

    await user.click(archiveHelpButton);
    const archiveHelp = await screen.findByRole("dialog", {
      name: "How Archive works",
    });
    expect(within(archiveHelp).getByText("Archive formats")).toBeVisible();
    expect(within(archiveHelp).getByText("ZIP")).toBeVisible();
    expect(within(archiveHelp).getByText("TAR.GZ")).toBeVisible();
    expect(within(archiveHelp).getByText("TAR.ZST")).toBeVisible();
    expect(
      within(archiveHelp).getByText("Limitations common to all formats"),
    ).toBeVisible();
    await user.click(
      within(archiveHelp).getByRole("button", { name: "Close" }),
    );
    await waitForElementToBeRemoved(archiveHelp);

    await user.click(switches[1]!);
    await waitFor(() =>
      expect(actionNumber).toHaveAttribute("data-enabled", "true"),
    );

    await user.click(switches[0]!);
    await waitFor(() => {
      expect(
        folderHeading.parentElement?.querySelector('[data-enabled="false"]'),
      ).toBeInTheDocument();
      expect(
        folderCard.querySelector('[data-enabled="false"]'),
      ).toBeInTheDocument();
    });

    const runActionButton = screen.getByRole("button", { name: "Run action" });
    const openActiveRunButton = screen.getByRole("button", {
      name: "Open active run",
    });
    expect(
      runActionButton.compareDocumentPosition(openActiveRunButton) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).not.toBe(0);
    expect(openActiveRunButton).toBeEnabled();
    expect(screen.getByRole("button", { name: "Stop" })).toBeEnabled();
    expect(screen.getByText("Queue position 1")).toBeInTheDocument();
  });

  it("uses the reusable Tree/List browser to toggle folders", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Folders" });

    await user.click(screen.getByRole("button", { name: "Add folders" }));
    const browser = await screen.findByTestId("folder-browser");
    expect(
      screen.getByRole("dialog", { name: "Folder browser" }),
    ).toContainElement(browser);
    expect(within(browser).getByRole("tree")).toBeInTheDocument();
    expect(
      await within(browser).findByRole("treeitem", { name: /Work.*Added/ }),
    ).toHaveAttribute("aria-selected", "true");

    const personal = within(browser).getByRole("treeitem", {
      name: "Personal",
    });
    await user.click(
      within(personal).getByRole("button", { name: "Expand folder" }),
    );
    await waitFor(() =>
      expect(personal).toHaveAttribute("aria-selected", "true"),
    );
    await user.keyboard(" ");
    expect(
      await within(browser).findByRole("treeitem", { name: /Personal.*Added/ }),
    ).toHaveAttribute("aria-selected", "true");
    await user.keyboard("=");
    expect(await within(browser).findByText("1.7 GiB")).toBeVisible();

    await user.click(
      within(browser).getByRole("button", { name: "Add to Favorites" }),
    );
    await user.click(within(browser).getByRole("button", { name: "Home" }));
    const favorite = browser.querySelector<HTMLButtonElement>(
      'button[title="C:\\\\Users\\\\Alice\\\\Documents\\\\Personal"]',
    );
    expect(favorite).not.toBeNull();
    await user.pointer({ keys: "[MouseRight]", target: favorite! });
    expect(
      within(browser).getByRole("menuitem", {
        name: "Remove from Favorites",
      }),
    ).toBeVisible();
    expect(
      within(browser).getByLabelText("C:\\Users\\Alice"),
    ).toBeInTheDocument();

    await user.keyboard("{Escape}");
    await user.click(within(browser).getByRole("button", { name: "D:\\" }));
    expect(
      await within(browser).findByRole("button", {
        name: "Collapse folder",
      }),
    ).toBeEnabled();
    expect(
      await within(browser).findByRole("treeitem", { name: "Projects" }),
    ).toBeVisible();
    await user.click(within(browser).getByRole("button", { name: "Home" }));

    await user.click(within(browser).getByRole("radio", { name: "List" }));
    expect(within(browser).getByRole("grid")).toBeInTheDocument();
    expect(within(browser).getByRole("row", { name: /\.\./ })).toBeVisible();

    await user.keyboard("{Home}{ArrowUp}");
    expect(document.activeElement).toHaveAttribute("role", "row");
    expect(document.activeElement).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{End}{ArrowDown}");
    expect(document.activeElement).toHaveAttribute("role", "row");
    expect(document.activeElement).toHaveAttribute("aria-selected", "true");

    await user.keyboard("=");
    const listSize = await within(browser).findByText("1.7 GiB");
    expect(listSize.closest(".mantine-Badge-root")).toBeNull();

    const closeButtons = within(browser).getAllByRole("button", {
      name: "Close",
    });
    expect(closeButtons).toHaveLength(1);
    await user.click(closeButtons[0]!);
    await user.click(screen.getByRole("button", { name: "Add folders" }));
    const reopenedBrowser = await screen.findByTestId("folder-browser");
    expect(
      within(reopenedBrowser).getByRole("radio", { name: "List" }),
    ).toBeChecked();
    expect(within(reopenedBrowser).getByRole("grid")).toBeInTheDocument();
  });

  it("uses the same folder browser for a custom archive output", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Folders" });

    await user.click(
      screen.getByRole("combobox", { name: "Output directory" }),
    );
    await user.keyboard("{ArrowDown}{Enter}");
    await user.click(
      await screen.findByRole("button", { name: "Choose folder" }),
    );

    const browser = await screen.findByTestId("folder-browser");
    expect(within(browser).getByText("Choose output folder")).toBeVisible();
    await user.click(
      within(browser).getByRole("button", { name: "Use selected folder" }),
    );

    expect(screen.getByLabelText("Custom output path")).toHaveValue(
      "C:\\Users\\Alice\\Documents",
    );
  });

  it("opens a paged preview and lazy run history from a folder card", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Folders" });

    await user.click(screen.getAllByRole("button", { name: "Preview" })[0]!);
    expect(await screen.findByText("Preview is ready")).toBeInTheDocument();
    expect(
      screen.getByRole("combobox", { name: "Action to preview" }),
    ).toHaveValue("Archive · Action 1");
    expect(screen.getByText(/Effective profile: Default/)).toBeInTheDocument();
    expect(screen.getByText("Before Ignore Profile")).toBeInTheDocument();
    expect(screen.getByText("Included size")).toBeInTheDocument();
    expect(screen.getByText("Excluded size")).toBeInTheDocument();
    expect(screen.getByText("2.29 GiB")).toBeInTheDocument();
    expect(screen.getByText("1.72 GiB")).toBeInTheDocument();
    expect(screen.getByText("579.5 MiB")).toBeInTheDocument();
    expect(screen.getByText("75.2%")).toBeInTheDocument();
    expect(screen.getByText("node_modules/react/index.js")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Run history" }));
    expect(
      await screen.findByRole("combobox", { name: "History filter" }),
    ).toBeInTheDocument();
    expect(
      await screen.findByText("Completed with warnings"),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("No runs for this folder"),
    ).not.toBeInTheDocument();

    await user.click(
      screen.getAllByRole("button", { name: /Archive.*Action 1/ })[0]!,
    );
    expect(await screen.findByText("Saved action")).toBeInTheDocument();
    expect(screen.getByText("Saved profile ID")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Run current settings" }),
    ).toBeEnabled();
  });
});
