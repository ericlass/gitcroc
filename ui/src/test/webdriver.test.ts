import { describe, expect, it } from "vitest";
import {
  driverPrograms,
  sessionCapabilities,
  smokeEnvironment,
  smokePaths,
  webviewEnvironment,
} from "../../webdriver/platform";

describe("PLAT-1 PLAT-2 ARCH-3 smoke driver platform setup", () => {
  it("finds native executables without a shell, including Windows paths with spaces", () => {
    expect(smokePaths("C:\\my project", "win32")).toEqual({
      app: "C:\\my project\\target\\e2e\\debug\\gitcroc.exe",
      cli: "C:\\my project\\target\\e2e\\debug\\gitcroc-cli.exe",
    });
    expect(smokePaths("/my project", "linux")).toEqual({
      app: "/my project/target/e2e/debug/gitcroc",
      cli: "/my project/target/e2e/debug/gitcroc-cli",
    });
    expect(driverPrograms("win32").map(([program]) => program)).toEqual(["tauri-driver.exe", "msedgedriver.exe"]);
    expect(driverPrograms("linux").map(([program]) => program)).toEqual(["tauri-driver", "WebKitWebDriver"]);
    expect(() => driverPrograms("darwin")).toThrow("Linux and Windows");
  });

  it("isolates Windows and Linux app config, cache and git config in the fixture", () => {
    const base = { APPDATA: "real config", LOCALAPPDATA: "real cache", GIT_CONFIG_GLOBAL: "real git config" };
    const windows = smokeEnvironment("C:\\fixture", base, "win32");
    expect(windows.GITCROC_E2E_DIR).toBe("C:\\fixture");
    // GITCROC_E2E_DIR isolates app data; preserve Windows' native profile environment for WebView2.
    expect(windows.APPDATA).toBe("real config");
    expect(windows.LOCALAPPDATA).toBe("real cache");
    expect(windows.GIT_CONFIG_GLOBAL).toBe("C:\\fixture\\home\\.gitconfig");
    expect(windows.GIT_CONFIG_NOSYSTEM).toBe("1");
    expect(windows.GIT_TERMINAL_PROMPT).toBe("0");
    expect(windows.LC_ALL).toBe("C");
    const linux = smokeEnvironment("/fixture", base, "linux");
    expect(linux.XDG_CONFIG_HOME).toBe("/fixture/config");
    expect(linux.XDG_CACHE_HOME).toBe("/fixture/cache");
    expect(linux.HOME).toBe("/fixture/home");
    expect(base.APPDATA).toBe("real config");
  });

  it("PLAT-1 attaches Edge to the real app on loopback without asking it to launch another app", () => {
    expect(sessionCapabilities("C:\\fixture\\gitcroc.exe", "win32")).toEqual({
      alwaysMatch: {
        browserName: "webview2",
        "ms:edgeChromium": true,
        "ms:edgeOptions": { debuggerAddress: "127.0.0.1:9222" },
      },
    });
    expect(sessionCapabilities("/fixture/gitcroc", "linux")).toEqual({
      alwaysMatch: { "tauri:options": { application: "/fixture/gitcroc" } },
    });
    const env = webviewEnvironment("C:\\fixture", { WEBVIEW2_BROWSER_EXECUTABLE_FOLDER: "runtime" }, "win32");
    expect(env.WEBVIEW2_USER_DATA_FOLDER).toBe("C:\\fixture\\webview");
    expect(env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS).toContain("--remote-debugging-port=9222");
    expect(env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS).toContain("--remote-debugging-address=127.0.0.1");
    expect(env.WEBVIEW2_BROWSER_EXECUTABLE_FOLDER).toBe("runtime");
  });
});
