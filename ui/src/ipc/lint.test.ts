// @vitest-environment node
import { fileURLToPath } from "node:url";
import { ESLint } from "eslint";
import { expect, test } from "vitest";

const eslint = new ESLint({ cwd: fileURLToPath(new URL("../..", import.meta.url)) });

async function errors(code: string, file: string): Promise<string[]> {
  const [result] = await eslint.lintText(code, { filePath: file });
  return (result?.messages ?? []).filter((m) => m.severity === 2).map((m) => m.ruleId ?? m.message);
}

test("components cannot call Tauri IPC directly", async () => {
  const file = "src/components/Example.tsx";
  expect(await errors(`import { invoke } from "@tauri-apps/api/core";\nvoid invoke("app_info");\n`, file)).toContain("no-restricted-imports");
  expect(await errors(`import { listen } from "@tauri-apps/api/event";\nvoid listen("job_progress", () => {});\n`, file)).toContain("no-restricted-imports");
  expect(await errors(`import { commands } from "../ipc/bindings";\nvoid commands.appInfo();\n`, file)).toContain("no-restricted-imports");
  expect(await errors(`void import("@tauri-apps/api/core");\n`, file)).toContain("no-restricted-syntax");
  expect(await errors(`void window.__TAURI_INTERNALS__;\n`, file)).toContain("no-restricted-syntax");
  expect(await errors(`import { appInfo } from "../ipc";\nvoid appInfo();\n`, file)).toEqual([]);
});
