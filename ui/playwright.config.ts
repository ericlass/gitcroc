import { defineConfig, devices } from "@playwright/test";

// Runs against a production build with the fake core (src/ipc/fake.ts),
// served by `vite preview`, Chromium only. WebDriver tests against the real
// window (tauri-driver) start in M1.
export default defineConfig({
  testDir: "e2e",
  forbidOnly: !!process.env.CI,
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
