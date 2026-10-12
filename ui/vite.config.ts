import { readFileSync } from "node:fs";
import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

// The app version has one source: `[workspace.package]` in the root Cargo.toml.
function workspaceVersion(): string {
  const manifest = readFileSync(new URL("../Cargo.toml", import.meta.url), "utf8");
  const match = /^\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/m.exec(manifest);
  if (!match?.[1]) {
    throw new Error("Cargo.toml has no [workspace.package] version");
  }
  return match[1];
}

// Port 1420 is the `devUrl` in crates/gitcroc-app/tauri.conf.json.
export default defineConfig({
  plugins: [solid()],
  define: {
    __APP_VERSION__: JSON.stringify(workspaceVersion()),
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2022",
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
