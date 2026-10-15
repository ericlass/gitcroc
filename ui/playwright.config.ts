import { defineConfig, devices } from "@playwright/test";

// Runs against a production build with the fake core (src/ipc/fake.ts),
// served by `vite preview`, Chromium only. WebDriver tests against the real
// window (tauri-driver) start in M1.
export default defineConfig({
  testDir: "e2e",
  forbidOnly: !!process.env.CI,
  // Limits that only end a hang: nothing here waits on a timer, and a loaded
  // runner can be many times slower than a desktop.
  timeout: 120_000,
  expect: { timeout: 30_000 },
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: "http://localhost:4173",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "pnpm exec vite build --mode fake && pnpm exec vite preview --mode fake --port 4173 --strictPort",
    url: "http://localhost:4173",
    reuseExistingServer: false,
  },
});
