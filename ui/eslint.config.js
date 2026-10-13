import js from "@eslint/js";
import { defineConfig } from "eslint/config";
import solid from "eslint-plugin-solid/configs/typescript";
import globals from "globals";
import tseslint from "typescript-eslint";

// IPC goes through src/ipc only, whose calls are generated (section 4
// "Stack": hand-written IPC is forbidden). These rules make any other route
// to Tauri a lint error.
const ipcOnly = {
  "no-restricted-imports": [
    "error",
    {
      patterns: [
        {
          group: ["@tauri-apps/api", "@tauri-apps/api/*"],
          message: "Call the core through src/ipc; its functions are generated.",
        },
        {
          group: ["**/ipc/bindings", "**/ipc/core", "**/ipc/fake"],
          message: "Import from src/ipc (index), not its internals.",
        },
      ],
    },
  ],
  "no-restricted-syntax": [
    "error",
    {
      selector: "ImportExpression[source.value=/^@tauri-apps\\u002Fapi/]",
      message: "Call the core through src/ipc; its functions are generated.",
    },
    {
      selector: "MemberExpression[property.name=/^__TAURI/]",
      message: "Call the core through src/ipc; its functions are generated.",
    },
  ],
};

export default defineConfig(
  { ignores: ["dist", "dist-fake", "dist-perf", "playwright-report", "test-results", "blob-report", "src/ipc/bindings.ts"] },
  js.configs.recommended,
  tseslint.configs.strict,
  {
    files: ["**/*.{ts,tsx}"],
    ...solid,
    languageOptions: {
      globals: { ...globals.browser },
    },
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    ignores: ["src/ipc/*"],
    rules: ipcOnly,
  },
  {
    // Tests drive the fake core directly.
    files: ["src/**/*.test.{ts,tsx}"],
    rules: { "no-restricted-imports": ["error", { patterns: [ipcOnly["no-restricted-imports"][1].patterns[0]] }] },
  },
  {
    files: ["*.config.{js,ts}", "bench/**/*.ts"],
    languageOptions: {
      globals: { ...globals.node },
    },
  },
);
