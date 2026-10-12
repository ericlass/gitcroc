import js from "@eslint/js";
import { defineConfig } from "eslint/config";
import solid from "eslint-plugin-solid/configs/typescript";
import globals from "globals";
import tseslint from "typescript-eslint";

export default defineConfig(
  { ignores: ["dist", "playwright-report", "test-results", "blob-report"] },
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
    files: ["*.config.{js,ts}"],
    languageOptions: {
      globals: { ...globals.node },
    },
  },
);
