// PLAT-1, PLAT-2: the smoke runner's platform paths and native drivers.
import { posix, win32 } from "node:path";

const paths = (platform: NodeJS.Platform) => platform === "win32" ? win32 : posix;

export const DEBUGGER = "127.0.0.1:9222";

/** Microsoft's documented attach mode avoids the current WebView2 launch regression. */
export function sessionCapabilities(app: string, platform: NodeJS.Platform) {
  return {
    alwaysMatch: platform === "win32"
      ? {
          browserName: "webview2",
          "ms:edgeChromium": true,
          "ms:edgeOptions": { debuggerAddress: DEBUGGER },
        }
      : { "tauri:options": { application: app } },
  };
}

export function smokePaths(root: string, platform: NodeJS.Platform) {
  const path = paths(platform);
  const suffix = platform === "win32" ? ".exe" : "";
  return {
    app: path.join(root, "target", "e2e", "debug", `gitcroc${suffix}`),
    cli: path.join(root, "target", "e2e", "debug", `gitcroc-cli${suffix}`),
  };
}

export function smokeEnvironment(dir: string, base: NodeJS.ProcessEnv, platform: NodeJS.Platform): NodeJS.ProcessEnv {
  const path = paths(platform);
  const home = path.join(dir, "home");
  const config = path.join(dir, "config");
  const cache = path.join(dir, "cache");
  return {
    ...base,
    GITCROC_E2E_DIR: dir,
    HOME: home,
    XDG_CONFIG_HOME: config,
    XDG_CACHE_HOME: cache,
    GIT_CONFIG_GLOBAL: path.join(home, ".gitconfig"),
    GIT_CONFIG_NOSYSTEM: "1",
    GIT_TERMINAL_PROMPT: "0",
    LC_ALL: "C",
  };
}

export function driverPrograms(platform: NodeJS.Platform): [[string, string], [string, string]] {
  if (platform !== "linux" && platform !== "win32") {
    throw new Error("WebDriver runs on Linux and Windows only; macOS has no WKWebView driver");
  }
  return [
    [platform === "win32" ? "tauri-driver.exe" : "tauri-driver", "cargo install tauri-driver --version 2.1.0 --locked"],
    platform === "win32"
      ? ["msedgedriver.exe", "install Microsoft Edge WebDriver matching the WebView2 runtime (CONTRIBUTING.md)"]
      : ["WebKitWebDriver", "sudo apt install webkit2gtk-driver (Debian, Ubuntu)"],
  ];
}
