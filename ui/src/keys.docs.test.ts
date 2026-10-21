// @vitest-environment node
// M5-18: the keyboard page of the user documentation lists every shortcut of
// keys.ts, as a table row with what it does and its keys on Windows and
// Linux and on macOS, written as the app writes them.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "vitest";
import { KEYMAP, type Shortcut, shortcutLabel } from "./keys";
import { strings } from "./strings/en";

const page = fileURLToPath(new URL("../../docs/user/keyboard.md", import.meta.url));

/** The cells of each table row of `markdown`, code spans unwrapped. */
function tableRows(markdown: string): string[][] {
  return markdown
    .split("\n")
    .filter((line) => line.trim().startsWith("|"))
    .map((line) =>
      line
        .trim()
        .replace(/^\||\|$/g, "")
        .split("|")
        .map((cell) => cell.trim().replace(/`/g, "")),
    );
}

test("keyboard.md lists every shortcut of keys.ts with its keys", () => {
  const rows = tableRows(readFileSync(page, "utf8"));
  const labels: Record<Shortcut, string> = strings.keys.shortcuts;
  for (const id of Object.keys(KEYMAP) as Shortcut[]) {
    const row = rows.find((cells) => cells[0] === labels[id]);
    expect(row, `no row for "${labels[id]}"`).toBeDefined();
    expect(row?.slice(1, 3), labels[id]).toEqual([shortcutLabel(id, false), shortcutLabel(id, true)]);
  }
});
