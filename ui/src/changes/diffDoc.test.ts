import { expect, test } from "vitest";
import { keep, textDiff } from "../ipc/fakeChanges";
import type { TextDiff } from "../ipc";
import { changeRanges, inlineDoc } from "./diffDoc";
import { fileRows, firstFile } from "./fileRows";

const text = (diff: ReturnType<typeof textDiff>): TextDiff => {
  if (diff.kind !== "text") {
    throw new Error("not a text diff");
  }
  return diff;
};

const ops = [...keep(1, 5), { remove: "line 6" }, { add: "line six" }, ...keep(7, 10), { add: "new" }, ...keep(17, 2)];

test("UI-17 inline diff is the core's hunks with both line numbers", () => {
  const diff = text(textDiff(ops, { ignore_whitespace: false, context: "three", load_anyway: false }));
  const doc = inlineDoc(diff);
  expect(doc.hunks.map((hunk) => hunk.header)).toEqual(["@@ -3,7 +3,7 @@", "@@ -14,5 +14,6 @@"]);
  const lines = doc.text.split("\n");
  expect(lines.slice(0, 5)).toEqual(["line 3", "line 4", "line 5", "line 6", "line six"]);
  expect(doc.kinds.slice(3, 5)).toEqual(["removed", "added"]);
  expect(doc.oldNumbers.slice(3, 5)).toEqual([6, null]);
  expect(doc.newNumbers.slice(3, 5)).toEqual([null, 6]);
  // The second hunk starts after the first one's lines.
  expect(doc.hunks[1]?.line).toBe(8);
});

test("UI-17 side-by-side ranges cover the changed lines of both whole files", () => {
  const diff = text(textDiff(ops, { ignore_whitespace: false, context: "whole_file", load_anyway: false }));
  const oldText = diff.old_text ?? "";
  const newText = diff.new_text ?? "";
  const ranges = changeRanges(diff, oldText, newText);
  expect(ranges.map((r) => [oldText.slice(r.fromA, r.toA), newText.slice(r.fromB, r.toB)])).toEqual([
    ["line 6\n", "line six\n"],
    ["", "new\n"],
  ]);
  // A pure insertion sits at the start of the old line it comes before.
  expect(oldText.slice(ranges[1]?.fromA).startsWith("line 17\n")).toBe(true);
});

test("UI-17 side-by-side ranges at the edges of a file without a final newline", () => {
  const diff: TextDiff = {
    added: 1,
    removed: 1,
    hunks: [
      {
        old_start: 1,
        old_lines: 2,
        new_start: 1,
        new_lines: 2,
        lines: [
          { kind: "context", old_line: 1, new_line: 1, text: "a" },
          { kind: "removed", old_line: 2, new_line: null, text: "b" },
          { kind: "added", old_line: null, new_line: 2, text: "c" },
        ],
      },
    ],
    old_text: "a\nb",
    new_text: "a\nc",
    old_missing_newline: true,
    new_missing_newline: true,
  };
  expect(changeRanges(diff, "a\nb", "a\nc")).toEqual([{ fromA: 2, toA: 3, fromB: 2, toB: 3 }]);
});

const file = (id: number, path: string) => ({
  id,
  path,
  old_path: null,
  status: "modified" as const,
  mode_change: null,
  added: 1,
  removed: 0,
});

test("UI-14 file tree folds single-folder chains and lists folders first", () => {
  const content = {
    kind: "commit" as const,
    details: { hash: "h", author: null as never, committer: null as never, message: "", parents: [] },
    parent: 0,
    files: [file(0, "README.md"), file(1, "src/deep/inner/a.rs"), file(2, "src/main.rs")],
  };
  const tree = fileRows(content, "tree", {});
  expect(tree.map((row) => (row.kind === "section" ? row.label : `${row.depth}:${row.name}`))).toEqual([
    "0:src",
    "1:deep/inner",
    "2:a.rs",
    "1:main.rs",
    "0:README.md",
  ]);
  // The first file differs between the modes, as the rows show it.
  expect(firstFile(content, "flat")).toEqual({ list: "commit", id: 0 });
  expect(firstFile(content, "tree")).toEqual({ list: "commit", id: 1 });
  // A closed folder hides what is inside.
  const closed = fileRows(content, "tree", { "commit:dir:src/": true });
  expect(closed.map((row) => (row.kind === "section" ? row.label : row.name))).toEqual(["src", "README.md"]);
});
