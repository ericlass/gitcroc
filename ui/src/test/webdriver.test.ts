import { describe, expect, it } from "vitest";
import { driverPrograms, smokeEnvironment, smokePaths } from "../../webdriver/platform";

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
    expect(windows.APPDATA).toBe("C:\\fixture\\config");
    expect(windows.LOCALAPPDATA).toBe("C:\\fixture\\cache");
    expect(windows.USERPROFILE).toBe("C:\\fixture\\home");
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
});
