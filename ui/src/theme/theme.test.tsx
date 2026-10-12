import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import App from "../App";
import { fakeState, reset } from "../ipc/fake";
import { setSystemDark } from "../test/media";

afterEach(() => {
  cleanup();
  reset();
  delete document.documentElement.dataset.theme;
});

const rootTheme = () => document.documentElement.dataset.theme;

async function openSettings() {
  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  return screen.findByRole("dialog", { name: "Settings" });
}

test("UI-23 theme switch: dark sets data-theme, system follows the OS live", async () => {
  render(() => <App />);
  await openSettings();
  expect(rootTheme()).toBe("light");

  fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
  await expect.poll(rootTheme).toBe("dark");
  expect(fakeState.settings.settings.theme).toBe("dark");

  // A fixed theme ignores the OS.
  setSystemDark(true);
  setSystemDark(false);
  expect(rootTheme()).toBe("dark");

  fireEvent.click(screen.getByRole("radio", { name: "Follow the system" }));
  await expect.poll(rootTheme).toBe("light");
  setSystemDark(true);
  expect(rootTheme()).toBe("dark");
  setSystemDark(false);
  expect(rootTheme()).toBe("light");
});

test("UI-23 saved theme applies on start", async () => {
  fakeState.settings = { settings: { theme: "dark" }, read_only: false, warning: null };
  render(() => <App />);
  await expect.poll(rootTheme).toBe("dark");
});

test("data_1_read_only_settings_show_a_banner_and_disable_the_choice", async () => {
  fakeState.settings = {
    settings: { theme: "light" },
    read_only: true,
    warning: { kind: "newer_schema", path: "/home/me/.config/gitcroc/settings.toml", found: 99, supported: 1 },
  };
  render(() => <App />);

  const banner = await screen.findByRole("alert");
  expect(banner.textContent).toContain("/home/me/.config/gitcroc/settings.toml");
  expect(banner.textContent).toContain("schema 99");
  expect(banner.textContent).toContain("read-only");

  await openSettings();
  // The fieldset disables the choices.
  expect(screen.getByRole("radio", { name: "Dark" }).matches(":disabled")).toBe(true);
  expect((screen.getByRole("radio", { name: "Light" }) as HTMLInputElement).checked).toBe(true);
});

test("sec_3_settings_warning_renders_as_text", async () => {
  fakeState.settings = {
    settings: { theme: "system" },
    read_only: true,
    warning: { kind: "unreadable", path: "/tmp/<b>x</b>/settings.toml", reason: "<img src=x onerror=alert(1)>" },
  };
  const { container } = render(() => <App />);
  const banner = await screen.findByRole("alert");
  expect(banner.textContent).toContain("<img src=x onerror=alert(1)>");
  expect(container.querySelector("b, img")).toBeNull();
});
