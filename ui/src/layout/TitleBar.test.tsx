import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import App from "../App";
import { fakeState, reset } from "../ipc/fake";
import { fakeWindow } from "../ipc/fakeWindow";

afterEach(() => {
  cleanup();
  reset();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const settleAll = async () => {
  for (let round = 0; round < 8; round++) {
    await settle();
  }
};

/** Renders the app on `os` and waits for its tabs. */
const start = async (os: string) => {
  fakeState.appInfo = { ...fakeState.appInfo, os };
  render(() => <App />);
  await screen.findByRole("tab", { name: "My workspace" });
  await settleAll();
};

const windowButtons = () => screen.queryByRole("group", { name: "Window controls" });
const tabBar = () => document.querySelector<HTMLElement>(".tab-bar");
const dragArea = () => document.querySelector<HTMLElement>(".title-drag");

test("ui_35_window_buttons_on_windows_and_linux_send_their_actions", async () => {
  for (const os of ["linux", "windows"]) {
    await start(os);
    const buttons = within(windowButtons() as HTMLElement);
    // Left out of the tab order, as the system's are.
    for (const button of buttons.getAllByRole("button")) {
      expect(button.getAttribute("tabindex")).toBe("-1");
    }
    fireEvent.click(buttons.getByRole("button", { name: "Minimize" }));
    fireEvent.click(buttons.getByRole("button", { name: "Maximize" }));
    // The maximize button shows restore while the window is maximized.
    expect(await buttons.findByRole("button", { name: "Restore" })).toBeTruthy();
    fireEvent.click(buttons.getByRole("button", { name: "Restore" }));
    expect(await buttons.findByRole("button", { name: "Maximize" })).toBeTruthy();
    fireEvent.click(buttons.getByRole("button", { name: "Close window" }));
    expect(fakeWindow.actions).toEqual(["minimize", "toggle_maximize", "toggle_maximize", "close"]);
    expect(tabBar()?.classList.contains("title-bar-mac")).toBe(false);
    cleanup();
    reset();
  }
});

test("ui_35_on_macos_the_system_buttons_stay_and_the_bar_makes_room", async () => {
  await start("macos");
  expect(windowButtons()).toBeNull();
  expect(tabBar()?.classList.contains("title-bar-mac")).toBe(true);
  // The empty space still moves the window.
  fireEvent.mouseDown(dragArea() as HTMLElement, { button: 0, detail: 1 });
  expect(fakeWindow.actions).toEqual(["start_dragging"]);
});

test("ui_35_the_empty_space_moves_the_window_and_a_double_click_maximizes", async () => {
  await start("linux");
  fireEvent.mouseDown(dragArea() as HTMLElement, { button: 0, detail: 1 });
  fireEvent.mouseDown(dragArea() as HTMLElement, { button: 0, detail: 2 });
  // Another button, or a press on a tab, leaves the window alone.
  fireEvent.mouseDown(dragArea() as HTMLElement, { button: 2, detail: 1 });
  fireEvent.mouseDown(screen.getByRole("tab", { name: "My workspace" }), { button: 0, detail: 1 });
  expect(fakeWindow.actions).toEqual(["start_dragging", "toggle_maximize"]);
});

test("ui_35_with_the_system_title_bar_the_tab_bar_is_only_tabs", async () => {
  fakeState.settings = {
    ...fakeState.settings,
    settings: { ...fakeState.settings.settings, system_title_bar: true },
  };
  await start("linux");
  expect(windowButtons()).toBeNull();
  fireEvent.mouseDown(dragArea() as HTMLElement, { button: 0, detail: 1 });
  expect(fakeWindow.actions).toEqual([]);
});

test("ui_35_the_setting_switches_the_title_bar_at_once", async () => {
  await start("linux");
  expect(windowButtons()).not.toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  const dialog = await screen.findByRole("dialog", { name: "Settings" });
  const box = within(dialog).getByRole("checkbox", { name: "Use the system title bar" }) as HTMLInputElement;
  expect(box.checked).toBe(false);
  fireEvent.click(box);
  await settleAll();
  expect(fakeState.settings.settings.system_title_bar).toBe(true);
  expect(windowButtons()).toBeNull();
});

test("ui_35_the_git_setup_screen_has_a_title_bar", async () => {
  fakeState.appInfo = { ...fakeState.appInfo, os: "windows" };
  fakeState.gitStatus = { kind: "not_found", searched: [], chosen: null, chosen_missing: false };
  render(() => <App />);
  await screen.findByRole("heading", { level: 1 });
  await settleAll();
  expect(document.querySelector(".title-bar-name")?.textContent).toBe("gitcroc");
  fireEvent.click(within(windowButtons() as HTMLElement).getByRole("button", { name: "Close window" }));
  fireEvent.mouseDown(document.querySelector(".title-bar") as HTMLElement, { button: 0, detail: 1 });
  expect(fakeWindow.actions).toEqual(["close", "start_dragging"]);
});
