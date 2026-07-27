import { render, screen } from "@testing-library/react";
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
      await screen.findByRole("heading", { name: "Configured tasks" }),
    ).toBeInTheDocument();
    expect(
      screen.getAllByText("C:\\Users\\Alice\\Documents\\Work").length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByText("D:\\Projects\\Work").length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "Run all" })).toBeEnabled();
  });

  it("routes between tasks and the profile workspace", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Configured tasks" });

    await user.click(screen.getByRole("button", { name: "Profiles" }));

    expect(
      await screen.findByRole(
        "heading",
        { name: "Default", level: 1 },
        { timeout: 5_000 },
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("default.packignore")).toBeInTheDocument();
  });

  it("creates a valid profile through the typed desktop command", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Configured tasks" });
    await user.click(screen.getByRole("button", { name: "Profiles" }));

    await user.click(
      (
        await screen.findAllByRole(
          "button",
          { name: "New profile" },
          { timeout: 5_000 },
        )
      )[0]!,
    );
    await user.type(screen.getByLabelText("Profile name"), "Release rules");
    await user.click(screen.getByRole("button", { name: "Confirm" }));

    expect(
      await screen.findByRole("heading", { name: "Release rules", level: 1 }),
    ).toBeInTheDocument();
    expect(screen.getByText("release-rules.packignore")).toBeInTheDocument();
  });

  it("switches the interface language without hardcoded component copy", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Configured tasks" });

    await user.click(screen.getByRole("button", { name: "Language" }));
    await user.click(await screen.findByText("Русский"));

    expect(
      screen.getByRole("heading", { name: "Настроенные задачи" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Запустить все" }),
    ).toBeInTheDocument();
  });

  it("offers an accessible light and dark theme control", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Configured tasks" });

    const toggle = screen.getByRole("button", {
      name: "Switch to dark theme",
    });
    await user.click(toggle);

    expect(
      screen.getByRole("button", { name: "Switch to light theme" }),
    ).toBeInTheDocument();
  });

  it("adds a folder-tree selection and controls an active run", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Configured tasks" });

    await user.click(screen.getByRole("button", { name: "Alice" }));
    expect(
      await screen.findByRole("button", {
        name: "Alice · C:\\Users\\Alice",
      }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Pause task" }));
    expect(
      await screen.findByRole("button", { name: "Resume task" }),
    ).toBeInTheDocument();
  });

  it("opens a paged preview and lazy run history from a task card", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: "Configured tasks" });

    await user.click(screen.getAllByRole("button", { name: "Preview" })[0]!);
    expect(await screen.findByText("Preview is ready")).toBeInTheDocument();
    expect(screen.getByText("node_modules/react/index.js")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Run history" }));
    expect(
      await screen.findByText("Completed with warnings"),
    ).toBeInTheDocument();
    expect(screen.queryByText("No runs for this task")).not.toBeInTheDocument();
  });
});
