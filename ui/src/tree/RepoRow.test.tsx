import { cleanup, render, screen } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import type { SummaryView } from "../ipc";
import { summary } from "../ipc/fakeTree";
import RepoRow from "./RepoRow";

afterEach(cleanup);

const clean = summary("fake-workspace", "r1", "main");

const withSummary = (change: Partial<NonNullable<SummaryView["summary"]>>, view: Partial<SummaryView> = {}) => {
  const base = clean.summary;
  if (!base) {
    throw new Error("the fake summary has no summary");
  }
  return { ...clean, ...view, summary: { ...base, ...change } };
};

const icons = () => screen.queryAllByRole("img").map((icon) => icon.getAttribute("aria-label"));

const row = (view: SummaryView | undefined, name = "api") => {
  const { container } = render(() => <RepoRow name={name} view={view} />);
  const element = container.querySelector(".tree-repo");
  if (!(element instanceof HTMLElement)) {
    throw new Error("no repo row");
  }
  return element;
};

test("UI-3 repo row: clean shows name and branch only", () => {
  const element = row(clean);
  expect(element.textContent).toBe("apimain");
  expect(icons()).toEqual([]);
  expect(element.classList.contains("stale")).toBe(false);
});

test("UI-3 repo row: dirty shows a dot", () => {
  row(withSummary({ dirty: true }));
  expect(icons()).toEqual(["Uncommitted changes"]);
});

test("UI-3 repo row: ahead and behind", () => {
  row(withSummary({ upstream: { name: "origin/main", ahead: 3, behind: 1, gone: false } }));
  expect(screen.getByLabelText("3 ahead").textContent).toBe("↑3");
  expect(screen.getByLabelText("1 behind").textContent).toBe("↓1");
});

test("UI-3 repo row: conflict state shows the conflict icon", () => {
  row(withSummary({ state: { operation: "merge", unmerged_paths: 2 } }, { conflict: true }));
  expect(icons()).toEqual(["Conflict state"]);
});

test("UI-3 repo row: a spinner while the summary is refreshed", () => {
  row({ ...clean, refreshing: true });
  expect(icons()).toEqual(["Refreshing"]);
});

test("UI-3 repo row: missing path shows a warning", () => {
  row({ ...clean, missing: true, stale: true });
  expect(icons()).toEqual(["Folder is missing"]);
});

test("UI-3 repo row: a summary from the cache is stale (PERF-3)", () => {
  expect(row({ ...clean, stale: true }).classList.contains("stale")).toBe(true);
  cleanup();
  // A repo never read has nothing but its name.
  const unread = row(undefined);
  expect(unread.textContent).toBe("api");
  expect(unread.classList.contains("stale")).toBe(true);
});

test("UI-3 repo row: a detached head shows its short hash", () => {
  row(withSummary({ head: { kind: "detached", hash: "89abcdef0123456789abcdef0123456789abcdef" } }));
  expect(screen.getByText("89abcde")).toBeDefined();
});

test("SEC-3 repo text is rendered as text", () => {
  const element = row(clean, "<img src=x onerror=alert(1)>");
  expect(element.querySelector("img")).toBeNull();
  expect(element.textContent).toContain("<img src=x onerror=alert(1)>");
});
