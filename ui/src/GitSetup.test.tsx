import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import App from "./App";
import { fakeState, reset } from "./ipc/fake";

afterEach(() => {
  cleanup();
  reset();
});

test("PLAT-3 setup screen: too old shows both versions, usable shows the app", async () => {
  fakeState.gitStatus = { kind: "too_old", path: "/usr/bin/git", found: "2.30.0", required: "2.40.0" };
  render(() => <App />);

  expect(await screen.findByRole("heading", { name: "gitcroc needs git" })).toBeDefined();
  expect(screen.getByRole("alert").textContent).toBe(
    "Found git 2.30.0 at /usr/bin/git, but gitcroc needs git 2.40.0 or newer.",
  );
  expect(screen.queryByRole("toolbar")).toBeNull();

  // "Check again" after installing a newer git shows the app, no restart.
  fakeState.gitStatus = { kind: "usable", path: "/usr/bin/git", version: "2.47.0" };
  fireEvent.click(screen.getByRole("button", { name: "Check again" }));
  expect(await screen.findByRole("toolbar", { name: "Main" })).toBeDefined();
  expect(screen.queryByRole("heading", { name: "gitcroc needs git" })).toBeNull();
});

test("PLAT-3 setup screen: not found lists the searched paths and an install hint", async () => {
  fakeState.appInfo = { ...fakeState.appInfo, os: "windows" };
  fakeState.gitStatus = { kind: "not_found", searched: ["C:\\Program Files\\Git\\cmd\\git.exe"] };
  render(() => <App />);

  expect(await screen.findByText("git was not found.")).toBeDefined();
  expect(screen.getByText("C:\\Program Files\\Git\\cmd\\git.exe")).toBeDefined();
  expect(await screen.findByText(/winget install --id Git\.Git/)).toBeDefined();
});

test("sec_3_git_path_and_output_render_as_text", async () => {
  const path = "/opt/<b>evil</b>/git";
  const output = "<img src=x onerror=alert(1)>";
  fakeState.gitStatus = { kind: "unparseable", path, output };
  const { container } = render(() => <App />);

  expect(await screen.findByText(output)).toBeDefined();
  expect(screen.getByRole("alert").textContent).toContain(path);
  expect(container.querySelector("b, img")).toBeNull();
});
