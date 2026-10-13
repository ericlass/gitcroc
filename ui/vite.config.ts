import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

// Port 1420 is the `devUrl` in crates/gitcroc-app/tauri.conf.json.
export default defineConfig(({ mode }) => {
  // The fake core (src/ipc/fake.ts) serves Vitest and `vite build --mode fake`,
  // which Playwright runs. Its build goes to dist-fake so it never ends up in
  // the bundle Tauri ships from dist.
  const fakeCore = mode === "fake" || !!process.env.VITEST;
  // `just bench-ui` builds with the UI-side measurements (M1-12) into
  // dist-perf, which only its perf-trace app embeds.
  const perfTrace = mode === "perf";
  return {
    // Hot reload is for `vite dev` only. Under Vitest its `@solid-refresh`
    // module fails to load on Windows.
    plugins: [solid({ hot: !process.env.VITEST })],
    define: {
      __FAKE_CORE__: JSON.stringify(fakeCore),
      __PERF_TRACE__: JSON.stringify(perfTrace),
    },
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
    },
    build: {
      target: "es2022",
      outDir: mode === "fake" ? "dist-fake" : perfTrace ? "dist-perf" : "dist",
    },
    test: {
      environment: "jsdom",
      include: ["src/**/*.test.{ts,tsx}"],
      setupFiles: ["src/test/setup.ts"],
    },
  };
});
