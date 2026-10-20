import { afterEach, expect, test } from "vitest";
import { conditions } from "./trace";

afterEach(() => document.body.replaceChildren());

test("PERF-8 the commit measurement waits for the current treegrid file rows and diff", () => {
  document.body.innerHTML = `
    <div class="commit-header"><span class="commit-hash">abc</span></div>
    <div class="file-list" role="treegrid">
      <div class="file-row file-row-section" role="row"><div role="gridcell">Files</div></div>
    </div>
    <div class="changes-diff"></div>`;
  const diff = document.querySelector(".changes-diff");
  const files = document.querySelector(".file-list");
  if (diff === null || files === null) {
    throw new Error("missing benchmark fixture");
  }
  expect(conditions.commit("abc")).toBe(false);
  diff.innerHTML = '<div class="cm-line">changed</div>';
  expect(conditions.commit("abc")).toBe(false);
  const file = document.createElement("div");
  file.className = "file-row file-row-file";
  file.setAttribute("role", "row");
  files.append(file);
  expect(conditions.commit("abc")).toBe(true);
  expect(conditions.commit("other")).toBe(false);
  diff.replaceChildren();
  expect(conditions.commit("abc")).toBe(false);
});
