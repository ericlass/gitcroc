import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import App from "./App";
import { fakeState, reset } from "./ipc/fake";

afterEach(() => {
  cleanup();
  reset();
});

test("PLAT-3 setup screen: too old shows both versions, usable shows the app", async () => {
  fakeState.gitStatus = {
    kind: "too_old",
    path: "/usr/bin/git",
    found: "2.30.0",
    required: "2.40.0",
    chosen: null,
    chosen_missing: false,
  };
  render(() => <App />);

  expect(await screen.findByRole("heading", { name: "gitcroc needs git" })).toBeDefined();
  expect(screen.getByRole("alert").textContent).toBe(
    "Found git 2.30.0 at /usr/bin/git, but gitcroc needs git 2.40.0 or newer.",
  );
  expect(screen.queryByRole("toolbar")).toBeNull();

  // "Check again" after installing a newer git shows the app, no restart.
  fakeState.gitStatus = {
    kind: "usable",
    path: "/usr/bin/git",
    version: "2.47.0",
    chosen: null,
    chosen_missing: false,
  };
  fireEvent.click(screen.getByRole("button", { name: "Check again" }));
  expect(await screen.findByRole("toolbar", { name: "Main" })).toBeDefined();
  expect(screen.queryByRole("heading", { name: "gitcroc needs git" })).toBeNull();
});

test("PLAT-3 setup screen: not found lists the searched paths and an install hint", async () => {
  fakeState.appInfo = { ...fakeState.appInfo, os: "windows" };
  fakeState.gitStatus = {
    kind: "not_found",
    searched: ["C:\\Program Files\\Git\\cmd\\git.exe"],
    chosen: null,
    chosen_missing: false,
  };
  render(() => <App />);

  expect(await screen.findByText("git was not found.")).toBeDefined();
  expect(screen.getByText("C:\\Program Files\\Git\\cmd\\git.exe")).toBeDefined();
  expect(await screen.findByText(/winget install --id Git\.Git/)).toBeDefined();
});

test("sec_3_git_path_and_output_render_as_text", async () => {
  const path = "/opt/<b>evil</b>/git";
  const output = "<img src=x onerror=alert(1)>";
  fakeState.gitStatus = { kind: "unparseable", path, output, chosen: null, chosen_missing: false };
  const { container } = render(() => <App />);

  expect(await screen.findByText(output)).toBeDefined();
  expect(screen.getByRole("alert").textContent).toContain(path);
  expect(container.querySelector("b, img")).toBeNull();
});

test("PLAT-3 setup screen without a chosen git: Choose git… gets past it", async () => {
  fakeState.gitStatus = { kind: "not_found", searched: ["/usr/bin/git"], chosen: null, chosen_missing: false };
  fakeState.gits = { "/opt/git/bin/git": "2.51.0" };
  fakeState.pickedGit = "/opt/git/bin/git";
  render(() => <App />);

  expect(await screen.findByRole("button", { name: "Check again" })).toBeDefined();
  expect(screen.queryByRole("button", { name: "Use automatic detection" })).toBeNull();
  expect(screen.queryByText(/The chosen git/)).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Choose git…" }));
  expect(await screen.findByRole("toolbar", { name: "Main" })).toBeDefined();
  expect(fakeState.gitPath).toBe("/opt/git/bin/git");
});

test("PLAT-3 setup screen with a chosen git: it says the path is missing and can go back", async () => {
  fakeState.gitStatus = { kind: "not_found", searched: ["/usr/bin/git"], chosen: null, chosen_missing: false };
  fakeState.gitPath = "/opt/gone/git";
  render(() => <App />);

  expect(await screen.findByText("The chosen git was not found at /opt/gone/git.")).toBeDefined();
  expect(screen.getByText("/opt/gone/git", { selector: "li" })).toBeDefined();

  // Automatic detection finds a git on PATH, so the app opens.
  fakeState.gitStatus = {
    kind: "usable",
    path: "/usr/bin/git",
    version: "2.47.0",
    chosen: null,
    chosen_missing: false,
  };
  fireEvent.click(screen.getByRole("button", { name: "Use automatic detection" }));
  expect(await screen.findByRole("toolbar", { name: "Main" })).toBeDefined();
  expect(fakeState.gitPath).toBeNull();
});

test("PLAT-3 setup screen: a chosen git that is too old is reported", async () => {
  fakeState.gits = { "/opt/old/git": "2.39.5" };
  fakeState.gitPath = "/opt/old/git";
  render(() => <App />);

  expect((await screen.findByRole("alert")).textContent).toBe(
    "Found git 2.39.5 at /opt/old/git, but gitcroc needs git 2.40.0 or newer.",
  );
  expect(screen.queryByText(/The chosen git was not found/)).toBeNull();
  expect(screen.getByRole("button", { name: "Use automatic detection" })).toBeDefined();
});

test("PLAT-3 Settings' Git section without a chosen git", async () => {
  fakeState.gits = { "/opt/git/bin/git": "2.51.0" };
  fakeState.pickedGit = "/opt/git/bin/git";
  render(() => <App />);
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  const section = await screen.findByRole("group", { name: "Git" });

  expect(section.textContent).toContain("git version 2.47.0 at /fake/bin/git");
  expect(screen.queryByRole("button", { name: "Use automatic detection" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Choose git…" }));
  expect(await screen.findByText("git version 2.51.0 at /opt/git/bin/git")).toBeDefined();
  expect(screen.getByRole("button", { name: "Use automatic detection" })).toBeDefined();
});

test("PLAT-3 Settings' Git section with a chosen git that is missing", async () => {
  fakeState.gitPath = "/opt/gone/git";
  render(() => <App />);
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  const section = await screen.findByRole("group", { name: "Git" });

  expect(section.textContent).toContain("git version 2.47.0 at /fake/bin/git");
  expect(screen.getByText("The chosen git was not found at /opt/gone/git; using /fake/bin/git.")).toBeDefined();
  fireEvent.click(screen.getByRole("button", { name: "Use automatic detection" }));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Use automatic detection" })).toBeNull());
  expect(screen.queryByText(/The chosen git was not found/)).toBeNull();
  expect(fakeState.gitPath).toBeNull();
});

test("PLAT-3 a cancelled git dialog changes nothing; read-only settings refuse the choice", async () => {
  render(() => <App />);
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  fireEvent.click(await screen.findByRole("button", { name: "Choose git…" }));
  await Promise.resolve();
  expect(fakeState.gitPath).toBeNull();
  expect(screen.getByText("git version 2.47.0 at /fake/bin/git")).toBeDefined();

  fakeState.settings = { ...fakeState.settings, read_only: true };
  fakeState.gitPath = "/opt/gone/git";
  cleanup();
  render(() => <App />);
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  const section = await screen.findByRole("group", { name: "Git" });
  expect(section.hasAttribute("disabled")).toBe(true);
});
