import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import App from "../App";
import { defaultAppearance, defaultChangesSettings, defaultGroupSettings, fakeState, reset } from "../ipc/fake";
import { setSystemDark } from "../test/media";
import { CODE_THEMES } from "./codeThemes";

afterEach(() => {
  cleanup();
  reset();
  delete document.documentElement.dataset.theme;
  document.documentElement.removeAttribute("style");
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
  fakeState.settings = {
    settings: { theme: "dark", ...defaultAppearance, ...defaultGroupSettings, changes: defaultChangesSettings },
    update_check: true,
    read_only: false,
    warning: null,
  };
  render(() => <App />);
  await expect.poll(rootTheme).toBe("dark");
});

test("data_1_read_only_settings_show_a_banner_and_disable_the_choice", async () => {
  fakeState.settings = {
    settings: { theme: "light", ...defaultAppearance, ...defaultGroupSettings, changes: defaultChangesSettings },
    update_check: true,
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
    settings: { theme: "system", ...defaultAppearance, ...defaultGroupSettings, changes: defaultChangesSettings },
    update_check: true,
    read_only: true,
    warning: { kind: "unreadable", path: "/tmp/<b>x</b>/settings.toml", reason: "<img src=x onerror=alert(1)>" },
  };
  const { container } = render(() => <App />);
  const banner = await screen.findByRole("alert");
  expect(banner.textContent).toContain("<img src=x onerror=alert(1)>");
  expect(container.querySelector("b, img")).toBeNull();
});

const rootVariable = (name: string) => document.documentElement.style.getPropertyValue(name);

test("UI-24 code theme follows the app theme", async () => {
  fakeState.settings = {
    ...fakeState.settings,
    settings: { ...fakeState.settings.settings, code_theme_light: "solarized_light", code_theme_dark: "dracula" },
  };
  render(() => <App />);
  await expect.poll(() => rootVariable("--code-bg")).toBe(CODE_THEMES.solarized_light.colors.background);
  expect(rootVariable("--diff-added-bg")).toBe(CODE_THEMES.solarized_light.colors.added);

  // The app theme switches the code theme between the two choices, live.
  setSystemDark(true);
  await expect.poll(() => rootVariable("--code-bg")).toBe(CODE_THEMES.dracula.colors.background);
  expect(rootVariable("--code-keyword")).toBe(CODE_THEMES.dracula.colors.keyword);
  setSystemDark(false);
  await expect.poll(() => rootVariable("--code-bg")).toBe(CODE_THEMES.solarized_light.colors.background);

  // Choosing another theme for the mode in use applies it at once and saves it.
  await openSettings();
  fireEvent.change(screen.getByRole("combobox", { name: "Code theme in light mode" }), {
    target: { value: "github_light" },
  });
  await expect.poll(() => rootVariable("--code-bg")).toBe(CODE_THEMES.github_light.colors.background);
  expect(fakeState.settings.settings.code_theme_light).toBe("github_light");
  expect(fakeState.settings.settings.code_theme_dark).toBe("dracula");
});

test("UI-24 each code theme choice shows its own preview", async () => {
  render(() => <App />);
  const dialog = await openSettings();
  fireEvent.change(screen.getByRole("combobox", { name: "Code theme in dark mode" }), { target: { value: "nord" } });
  await expect.poll(() => fakeState.settings.settings.code_theme_dark).toBe("nord");
  const previews = [...dialog.querySelectorAll<HTMLElement>(".code-preview")];
  expect(previews.map((preview) => preview.style.getPropertyValue("--code-bg"))).toEqual([
    CODE_THEMES.github_light.colors.background,
    CODE_THEMES.nord.colors.background,
  ]);
  // The app itself stays light.
  expect(rootVariable("--code-bg")).toBe(CODE_THEMES.github_light.colors.background);
});

test("UI-26 fonts apply", async () => {
  render(() => <App />);
  await openSettings();
  await expect.poll(() => rootVariable("--font-code")).toBe("var(--font-code-system)");
  expect(rootVariable("--font-size-code")).toBe("13px");

  const font = screen.getByRole("textbox", { name: "Code font" });
  expect(font.getAttribute("placeholder")).toBe("System monospace font");
  fireEvent.change(font, { target: { value: 'Fira "Code"' } });
  await expect.poll(() => rootVariable("--font-code")).toBe('"Fira \\"Code\\"", var(--font-code-system)');
  fireEvent.change(screen.getByRole("spinbutton", { name: "Code font size" }), { target: { value: "40" } });
  await expect.poll(() => rootVariable("--font-size-code")).toBe("32px");

  fireEvent.change(screen.getByRole("textbox", { name: "UI font" }), { target: { value: "Inter" } });
  await expect.poll(() => rootVariable("--font-ui")).toBe('"Inter", var(--font-ui-system)');
  fireEvent.change(screen.getByRole("spinbutton", { name: "UI font size" }), { target: { value: "15" } });
  await expect.poll(() => rootVariable("--font-size-ui")).toBe("15px");
  expect(fakeState.settings.settings).toMatchObject({
    ui_font: "Inter",
    ui_font_size: 15,
    code_font: 'Fira "Code"',
    code_font_size: 32,
  });

  // Clearing a font goes back to the system font.
  fireEvent.change(screen.getByRole("textbox", { name: "Code font" }), { target: { value: " " } });
  await expect.poll(() => rootVariable("--font-code")).toBe("var(--font-code-system)");
});

test("UI-26 the desktop's fonts come first in the system fonts", async () => {
  fakeState.appInfo = { ...fakeState.appInfo, ui_font: 'Ubuntu "Sans"', code_font: "JetBrains Mono NL" };
  render(() => <App />);
  await expect.poll(() => rootVariable("--font-code-system")).toBe('"JetBrains Mono NL", var(--font-code-stack)');
  await expect.poll(() => rootVariable("--font-ui-system")).toBe('"Ubuntu \\"Sans\\"", var(--font-ui-stack)');
  // A font set by the user still comes before them.
  await openSettings();
  fireEvent.change(screen.getByRole("textbox", { name: "UI font" }), { target: { value: "Inter" } });
  await expect.poll(() => rootVariable("--font-ui")).toBe('"Inter", var(--font-ui-system)');
});

test("UI-26 without desktop fonts the CSS lists are the system fonts", async () => {
  render(() => <App />);
  await screen.findByRole("button", { name: "Settings" });
  expect(rootVariable("--font-ui-system")).toBe("var(--font-ui-stack)");
  expect(rootVariable("--font-code-system")).toBe("var(--font-code-stack)");
});
