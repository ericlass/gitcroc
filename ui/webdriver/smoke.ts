// `just smoke` (section 12 "End to end smoke", M2-15): the real app, built
// with the `e2e` feature, on a fixture workspace through tauri-driver and
// WebKitWebDriver, Linux only. It adds a repo, stages a file, commits,
// merges a branch that conflicts, resolves the file in the conflict editor
// with Take theirs and an edit, marks it resolved and continues; the
// history then shows the merge commit and the repo is clean. Then the group
// flow (M3-15): a second workspace tab gets both fixture repos, a group of
// them made from a multi-selection, and a group checkout of a branch only
// one of them has, which creates it in the other. Last the AI flow
// (M4-12): a merge that conflicts again, a mock provider added in Settings,
// AI features off and on, and the file resolved with AI: consent, the
// proposal, Accept all high-confidence, Mark resolved and Continue. The
// mock answers from scripted replies (`GITCROC_E2E_AI`); nothing calls a
// real provider.
//
// No step waits for a time: each waits in the page for its result in the
// DOM, up to a deadline that only ends a hung run.
//
// Node runs this file directly (type stripping), so it uses no TypeScript
// syntax that needs compiling, and no packages.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { checkDrivers, ELEMENT, type Element, log, startDriver, wd } from "./client.ts";

const ROOT = resolve(import.meta.dirname, "../..");
const APP = join(ROOT, "target/e2e/debug/gitcroc");
const CLI = join(ROOT, "target/e2e/debug/gitcroc-cli");
/** Ends a step that never sees its result; no step is expected to come near it. */
const DEADLINE_MS = 60_000;

// --- The fixture ------------------------------------------------------------

/** git as the fixture runs it: no user or system config, a fixed identity. */
function git(env: NodeJS.ProcessEnv, repo: string, args: string[]): string {
  return execFileSync("git", ["-C", repo, ...args], { env, encoding: "utf8" });
}

/**
 * `lib`, in the workspace from the start, and `app`, added by the test:
 * `notes.txt` to stage and commit, and `main`, `feature` and `topic` that
 * each changed `a.txt`.
 */
function fixture(dir: string, env: NodeJS.ProcessEnv) {
  const lib = join(dir, "lib");
  const app = join(dir, "app");
  for (const repo of [lib, app]) {
    mkdirSync(repo);
    git(env, repo, ["init", "--quiet", "--initial-branch=main"]);
    git(env, repo, ["config", "user.name", "Smoke Test"]);
    git(env, repo, ["config", "user.email", "smoke@example.com"]);
  }
  writeFileSync(join(lib, "lib.txt"), "lib\n");
  git(env, lib, ["add", "lib.txt"]);
  git(env, lib, ["commit", "--quiet", "-m", "Lib"]);

  writeFileSync(join(app, "a.txt"), "base\n");
  git(env, app, ["add", "a.txt"]);
  git(env, app, ["commit", "--quiet", "-m", "Base"]);
  git(env, app, ["switch", "--quiet", "-c", "feature"]);
  writeFileSync(join(app, "a.txt"), "feature\n");
  git(env, app, ["commit", "--quiet", "-am", "Feature"]);
  git(env, app, ["switch", "--quiet", "-c", "topic", "main"]);
  writeFileSync(join(app, "a.txt"), "topic\n");
  git(env, app, ["commit", "--quiet", "-am", "Topic"]);
  git(env, app, ["switch", "--quiet", "main"]);
  writeFileSync(join(app, "a.txt"), "main\n");
  git(env, app, ["commit", "--quiet", "-am", "Main"]);
  writeFileSync(join(app, "notes.txt"), "notes\n");
  return { lib, app };
}

// --- The page ---------------------------------------------------------------

/**
 * Finds elements by role and accessible name, as Testing Library does, in
 * the page. `path` is a list of [role, name] steps, each searched in the
 * one before; a name `/.../` is a regular expression. Only visible
 * elements count.
 */
const FIND = `
  const ROLES = {
    alertdialog: '[role="alertdialog"]',
    button: 'button,[role="button"]',
    checkbox: 'input[type="checkbox"],[role="checkbox"]',
    combobox: 'input[list],select,[role="combobox"]',
    dialog: '[role="dialog"],dialog',
    listbox: '[role="listbox"]',
    menu: '[role="menu"]',
    menuitem: '[role="menuitem"]',
    option: '[role="option"]',
    radio: 'input[type="radio"],[role="radio"]',
    region: 'section[aria-label],[role="region"]',
    textbox: 'input:not([type]),input[type="text"],textarea,[role="textbox"]',
    toolbar: '[role="toolbar"]',
    tree: '[role="tree"]',
    treeitem: '[role="treeitem"]',
  };
  const nameOf = (el) => {
    const label = el.getAttribute("aria-label");
    if (label) return label.trim();
    const by = el.getAttribute("aria-labelledby");
    if (by) {
      return by.split(/\\s+/).map((id) => document.getElementById(id)?.textContent ?? "").join(" ").trim();
    }
    if (el.labels && el.labels.length > 0) return el.labels[0].textContent.trim();
    return textOf(el).trim();
  };
  // Text without what is hidden from assistive technology, such as a row's arrow.
  const textOf = (node) => {
    if (node.nodeType === Node.TEXT_NODE) return node.textContent;
    if (node.nodeType !== Node.ELEMENT_NODE || node.getAttribute("aria-hidden") === "true") return "";
    return [...node.childNodes].map(textOf).join("");
  };
  const matches = (name, pattern) => {
    if (pattern.startsWith("/") && pattern.endsWith("/")) return new RegExp(pattern.slice(1, -1)).test(name);
    return name === pattern;
  };
  const find = (path) => {
    let roots = [document];
    for (const [role, pattern] of path) {
      const found = [];
      for (const root of roots) {
        for (const el of root.querySelectorAll(ROLES[role])) {
          if (el.getClientRects().length > 0 && matches(nameOf(el), pattern)) found.push(el);
        }
      }
      if (found.length === 0) return null;
      roots = found;
    }
    return roots[0];
  };
  const enabled = (el) => !el.disabled && el.getAttribute("aria-disabled") !== "true";
`;

type Path = [string, string][];

class Session {
  readonly id: string;

  constructor(id: string) {
    this.id = id;
  }

  static async start(app: string): Promise<Session> {
    const value = (await wd("POST", "/session", {
      capabilities: { alwaysMatch: { "tauri:options": { application: app } } },
    })) as { sessionId: string };
    const session = new Session(value.sessionId);
    await wd("POST", `/session/${session.id}/timeouts`, { script: DEADLINE_MS + 10_000 });
    return session;
  }

  /**
   * Waits until `check`, the body of a function given `find` and `args`,
   * returns something other than null, false or undefined, and returns it:
   * checked again on every frame, with no fixed wait.
   */
  async until<T>(what: string, check: string, ...args: unknown[]): Promise<T> {
    log(`  waiting for ${what}`);
    const script = `
      const done = arguments[arguments.length - 1];
      const args = Array.prototype.slice.call(arguments, 0, -1);
      ${FIND}
      const check = () => { ${check} };
      const since = performance.now();
      const step = () => {
        let value;
        try {
          value = check();
        } catch (error) {
          done({ error: String((error && error.message) || error) });
          return;
        }
        if (value !== null && value !== undefined && value !== false) {
          done({ ok: value });
        } else if (performance.now() - since > ${DEADLINE_MS}) {
          done({ timeout: true });
        } else {
          requestAnimationFrame(step);
        }
      };
      step();`;
    const value = (await wd("POST", `/session/${this.id}/execute/async`, { script, args })) as {
      ok?: T;
      error?: string;
      timeout?: boolean;
    };
    if (value.error !== undefined) {
      throw new Error(`${what}: ${value.error}`);
    }
    if (value.timeout === true) {
      throw new Error(`never saw ${what}`);
    }
    return value.ok as T;
  }

  /** The element at `path`, once it is shown and enabled. */
  element(path: Path): Promise<Element> {
    return this.until<Element>(
      path.map(([role, name]) => `${role} "${name}"`).join(" in "),
      `const el = find(args[0]); return el !== null && enabled(el) ? el : null;`,
      path,
    );
  }

  async click(path: Path) {
    const element = await this.element(path);
    await wd("POST", `/session/${this.id}/element/${element[ELEMENT]}/click`, {});
  }

  /** Types `text` into the element at `path`; WebDriver key codes work too. */
  async type(path: Path, text: string) {
    const element = await this.element(path);
    await wd("POST", `/session/${this.id}/element/${element[ELEMENT]}/value`, { text });
  }

  /** Waits until the text of the elements matching `css` is `expected`. */
  textIs(css: string, expected: string[]) {
    return this.until(
      `${css} to read ${JSON.stringify(expected)}`,
      `const texts = [...document.querySelectorAll(args[0])].map((el) => el.textContent.trim());
       return JSON.stringify(texts) === JSON.stringify(args[1]);`,
      css,
      expected,
    );
  }

  /** Waits until nothing is at `path`. */
  gone(path: Path) {
    return this.until(
      `no ${path.map(([role, name]) => `${role} "${name}"`).join(" in ")}`,
      `return find(args[0]) === null;`,
      path,
    );
  }

  /** Clicks the element matching `css`, once it is shown and enabled. */
  async clickCss(css: string) {
    const element = await this.until<Element>(
      css,
      `const el = [...document.querySelectorAll(args[0])].find((el) => el.getClientRects().length > 0);
       return el !== undefined && enabled(el) ? el : null;`,
      css,
    );
    await wd("POST", `/session/${this.id}/element/${element[ELEMENT]}/click`, {});
  }

  /**
   * Clicks the element at `path` with the mouse: with `key` held down, or
   * with the right button for its context menu.
   */
  async pointer(path: Path, how: { key?: string; button?: 0 | 2 }) {
    const element = await this.element(path);
    const button = how.button ?? 0;
    const mouse = {
      type: "pointer",
      id: "mouse",
      parameters: { pointerType: "mouse" },
      actions: [
        { type: "pause" },
        { type: "pointerMove", origin: { [ELEMENT]: element[ELEMENT] }, x: 0, y: 0 },
        { type: "pointerDown", button },
        { type: "pointerUp", button },
        { type: "pause" },
      ],
    };
    const keyboard = {
      type: "key",
      id: "keyboard",
      // Down before the pointer moves, up after it is released.
      actions:
        how.key === undefined
          ? []
          : [
              { type: "keyDown", value: how.key },
              { type: "pause" },
              { type: "pause" },
              { type: "pause" },
              { type: "keyUp", value: how.key },
            ],
    };
    await wd("POST", `/session/${this.id}/actions`, { actions: [mouse, keyboard] });
    await wd("DELETE", `/session/${this.id}/actions`);
  }

  /** Presses each chord of keys in turn, as a user types: all down, then all up. */
  async keys(chords: string[][]) {
    const actions = chords.flatMap((chord) => [
      ...chord.map((value) => ({ type: "keyDown", value })),
      ...[...chord].reverse().map((value) => ({ type: "keyUp", value })),
    ]);
    await wd("POST", `/session/${this.id}/actions`, { actions: [{ type: "key", id: "keyboard", actions }] });
  }

  /** Picks the option `value` of the select at `path`, with a click on the option as a user does. */
  async select(path: Path, value: string) {
    const select = await this.element(path);
    const option = await this.until<Element>(
      `option ${value}`,
      `const el = find(args[0]);
       return el === null ? null : [...el.options].find((option) => option.value === args[1]) ?? null;`,
      path,
      value,
    );
    await wd("POST", `/session/${this.id}/element/${select[ELEMENT]}/click`, {});
    await wd("POST", `/session/${this.id}/element/${option[ELEMENT]}/click`, {});
    await this.until(`${value} picked`, `return find(args[0])?.value === args[1];`, path, value);
  }

  async screenshot(path: string) {
    const png = (await wd("GET", `/session/${this.id}/screenshot`)) as string;
    writeFileSync(path, Buffer.from(png, "base64"));
  }

  end() {
    return wd("DELETE", `/session/${this.id}`);
  }
}

// --- The run ----------------------------------------------------------------

/** WebDriver key codes (W3C WebDriver, "Keyboard actions"). */
const KEY = { release: "", control: "", home: "", end: "" };

const TREE: Path = [["tree", "Repositories"]];
const HISTORY: Path = [["listbox", "History"]];
const BANNER: Path = [["region", "Conflict state"]];
const CONFLICT: Path = [["region", "Conflict"]];
const FILE_SECTIONS = '[role="listbox"][aria-label="Changed files"] .file-section';

async function smoke(session: Session, app: string, env: NodeJS.ProcessEnv) {
  log("the app starts with the fixture workspace");
  await session.element([...TREE, ["treeitem", "/^lib/"]]);

  log("add a repo");
  await session.click([["button", "Add repository"]]);
  await session.click([
    ["menu", "Add repository"],
    ["menuitem", "Add folder…"],
  ]);
  await session.click([...TREE, ["treeitem", "/^app/"]]);
  await session.click([...HISTORY, ["option", "/Uncommitted changes/"]]);

  log("stage a file");
  await session.textIs(FILE_SECTIONS, ["Staged (0)", "Unstaged (1)"]);
  // The list may be read again while the repo just added settles; a click on
  // the list it replaced reaches nothing, the app says so, and the user
  // clicks again.
  for (let attempt = 1; ; attempt++) {
    await session.click([["button", "Stage notes.txt"]]);
    const outcome = await session.until<string>(
      "the file staged, or the list read again",
      `const texts = [...document.querySelectorAll(args[0])].map((el) => el.textContent.trim());
       if (JSON.stringify(texts) === JSON.stringify(args[1])) return "staged";
       const notices = [...document.querySelectorAll('[role="status"]')].map((el) => el.textContent.trim());
       return notices.includes(args[2]) ? "again" : null;`,
      FILE_SECTIONS,
      ["Staged (1)", "Unstaged (0)"],
      "The list changed; showing it again.",
    );
    if (outcome === "staged") {
      break;
    }
    if (attempt === 3) {
      throw new Error("the list changed under every click on Stage");
    }
  }

  log("commit");
  await session.type(
    [
      ["region", "Commit message"],
      ["textbox", "Subject"],
    ],
    "Add notes",
  );
  // As for Stage: a click on a list read again meanwhile is refused and said.
  for (let attempt = 1; ; attempt++) {
    await session.click([
      ["region", "Commit message"],
      ["button", "Commit"],
    ]);
    const outcome = await session.until<string>(
      "the commit in the history, or the list read again",
      `const rows = [...document.querySelectorAll('[role="listbox"][aria-label="History"] [role="option"]')];
       if (rows.some((row) => row.textContent.includes(args[0]))) return "committed";
       const alerts = [...document.querySelectorAll('[role="alert"]')].map((el) => el.textContent.trim());
       return alerts.includes(args[1]) ? "again" : null;`,
      "Add notes",
      "The list changed; showing it again.",
    );
    if (outcome === "committed") {
      break;
    }
    if (attempt === 3) {
      throw new Error("the list changed under every click on Commit");
    }
  }

  log("merge a branch that conflicts");
  await session.click([
    ["toolbar", "Main"],
    ["button", "Merge"],
  ]);
  const dialog: Path = [["dialog", "Merge into main in app"]];
  await session.type([...dialog, ["textbox", "Branch, remote branch or tag"]], "feature");
  await session.click([...dialog, ["option", "feature"]]);
  await session.click([...dialog, ["button", "Merge"]]);
  await session.element(BANNER);

  log("resolve it in the conflict editor: Take theirs and an edit");
  await session.click([...HISTORY, ["option", "/Uncommitted changes/"]]);
  await session.click([...CONFLICT, ["button", "Take theirs in conflict 1"]]);
  await session.textIs('section[aria-label="Conflict"] .cm-content', ["feature"]);
  const editor = await session.until<Element>(
    "the conflict editor",
    `return document.querySelector('section[aria-label="Conflict"] .cm-content');`,
  );
  await wd("POST", `/session/${session.id}/element/${editor[ELEMENT]}/click`, {});
  await session.keys([[KEY.control, KEY.home], [KEY.end], ..." and an edit".split("").map((key) => [key])]);
  await session.textIs('section[aria-label="Conflict"] .cm-content', ["feature and an edit"]);

  log("Mark resolved, Continue");
  await session.click([...CONFLICT, ["button", "Mark resolved"]]);
  await session.until(
    "all conflicts resolved",
    `const banner = find(args[0]);
     return banner !== null && banner.textContent.includes("All conflicts are resolved and staged.");`,
    BANNER,
  );
  await session.click([...BANNER, ["button", "Continue"]]);
  await session.gone(BANNER);

  log("the history shows the merge commit and the repo is clean");
  await session.element([...HISTORY, ["option", "/Merge branch 'feature'/"]]);
  await session.gone([...HISTORY, ["option", "/Uncommitted changes/"]]);
  const status = git(env, app, ["status", "--porcelain", "--untracked-files=all"]);
  if (status !== "") {
    throw new Error(`the repo is not clean:\n${status}`);
  }
  const parents = git(env, app, ["rev-list", "--parents", "-n", "1", "HEAD"]).trim().split(" ").length - 1;
  if (parents !== 2) {
    throw new Error(`HEAD has ${parents} parents, not 2`);
  }
  const merged = readFileSync(join(app, "a.txt"), "utf8");
  if (merged !== "feature and an edit\n") {
    throw new Error(`a.txt is ${JSON.stringify(merged)}`);
  }
  const notes = git(env, app, ["log", "--format=%s", "-n", "3", "--first-parent"]).trim().split("\n");
  if (JSON.stringify(notes) !== JSON.stringify(["Merge branch 'feature'", "Add notes", "Main"])) {
    throw new Error(`the history is ${JSON.stringify(notes)}`);
  }
}

/**
 * The group flow: a second workspace tab with both fixture repos, a group of
 * them from a multi-selection, and a group checkout of `feature`, which
 * only `app` has, with the policy "create it from HEAD": the preflight
 * shows `lib` as needing attention, and the report has an outcome for each.
 */
async function groupFlow(session: Session, env: NodeJS.ProcessEnv, repos: { lib: string; app: string }) {
  log("create a second workspace tab");
  await session.click([["button", "Open workspace"]]);
  await session.click([
    ["menu", "Open workspace"],
    ["menuitem", "New workspace…"],
  ]);
  await session.type(
    [
      ["dialog", "New workspace"],
      ["textbox", "/^Name/"],
    ],
    "Second",
  );
  await session.click([
    ["dialog", "New workspace"],
    ["button", "Create"],
  ]);
  await session.until(
    "the tab Second, active",
    `return [...document.querySelectorAll('[role="tab"]')].some(
       (tab) => tab.textContent.startsWith("Second") && tab.getAttribute("aria-selected") === "true",
     );`,
  );

  log("add both fixture repos");
  await session.click([["button", "Add repository"]]);
  await session.click([
    ["menu", "Add repository"],
    ["menuitem", "Add folder…"],
  ]);
  const lib: Path = [...TREE, ["treeitem", "/^lib/"]];
  const app: Path = [...TREE, ["treeitem", "/^app/"]];
  await session.element(lib);
  await session.element(app);

  log("make a group of both from a multi-selection");
  await session.click(lib);
  await session.pointer(app, { key: KEY.control });
  await session.until(
    "both repos selected",
    `return [...document.querySelectorAll('[role="tree"] [role="treeitem"][aria-selected="true"]')].length === 2;`,
  );
  await session.pointer(app, { button: 2 });
  await session.click([
    ["menu", "Repository"],
    ["menuitem", "Add to group ▸"],
  ]);
  await session.click([
    ["menu", "Add to group"],
    ["menuitem", "New group…"],
  ]);
  await session.type(
    [
      ["dialog", "New group"],
      ["textbox", "/^Name/"],
    ],
    // Lower case: after the Ctrl-click's key actions, WebKitWebDriver typed
    // "Pair" as "pair".
    "both",
  );
  await session.click([
    ["dialog", "New group"],
    ["button", "Create"],
  ]);
  const group: Path = [...TREE, ["treeitem", "/^both/"]];
  await session.until(
    "the group both with 2 members",
    `const row = find(args[0]);
     return row !== null && row.querySelector(".tree-group .tree-count")?.textContent === "2";`,
    group,
  );

  log("group checkout of feature, which only app has; lib creates it from HEAD");
  await session.pointer(group, { button: 2 });
  await session.click([
    ["menu", "Group"],
    ["menuitem", "Check out branch…"],
  ]);
  const checkout: Path = [["dialog", "Check out a branch in both"]];
  await session.type([...checkout, ["combobox", "Branch"]], "feature");
  await session.click([...checkout, ["radio", "Create the branch from HEAD"]]);
  await session.click([...checkout, ["button", "Check out"]]);

  log("the preflight shows lib as needing attention; Run");
  const preflight: Path = [["dialog", "/both/"]];
  await session.until(
    "lib needs attention and app will run",
    `const dialog = find(args[0]);
     if (dialog === null) return null;
     const rows = [...dialog.querySelectorAll(".preflight-repo")].map(
       (row) =>
         row.querySelector(".preflight-name").textContent + ": " + row.querySelector(".preflight-class").textContent,
     );
     return JSON.stringify(rows) === JSON.stringify(["lib: Needs attention", "app: Will run"]);`,
    preflight,
  );
  await session.click([...preflight, ["button", "Run on 2 repos"]]);
  await session.gone(preflight);

  log("the report has two outcomes, and the group row shows the branch");
  await session.until(
    "the group row on feature",
    `return find(args[0])?.querySelector(".tree-group .tree-head")?.textContent === "feature";`,
    group,
  );
  await session.clickCss(".status-bar .status-jobs");
  await session.click([
    ["dialog", "Jobs"],
    ["option", "/^Checkout feature/"],
  ]);
  await session.until(
    "a report line for each repo",
    `const lines = [...document.querySelectorAll('[aria-label="Result report"] .report-line')];
     const texts = lines.map((line) => line.querySelector(".report-name").textContent + ": " + line.textContent);
     return lines.length === 2 && texts.every((text) => text.includes("Succeeded")) ? texts : null;`,
  );
  await session.click([
    ["dialog", "Jobs"],
    ["button", "Close"],
  ]);
  for (const [name, repo] of Object.entries(repos)) {
    const branch = git(env, repo, ["branch", "--show-current"]).trim();
    if (branch !== "feature") {
      throw new Error(`${name} is on ${branch}, not feature`);
    }
  }
}

/** The mock's one scripted reply: hunk 1 resolved with high confidence. */
const AI_REPLY = JSON.stringify({
  hunks: [
    {
      id: 1,
      status: "resolved",
      resolution: "feature and topic\n",
      explanation: "Keeps the line of both sides.",
      confidence: "high",
    },
  ],
});

const SETTINGS: Path = [["dialog", "Settings"]];

/** Opens Settings, runs `steps` in it and closes it. */
async function inSettings(session: Session, steps: () => Promise<void>) {
  await session.click([["button", "Settings"]]);
  await session.element(SETTINGS);
  await steps();
  await session.click([...SETTINGS, ["button", "Close"]]);
  await session.gone(SETTINGS);
}

/**
 * The AI flow (M4-12), in the second tab: `app`, on `feature` since the
 * group checkout, merges `topic`, which conflicts in `a.txt`. A mock
 * provider is added and made the default; with AI features off the editor
 * has no Resolve with AI (AI-16); on again, Resolve with AI asks for
 * consent (AI-6), the proposal is accepted (AI-11) and only Mark resolved
 * and Continue write it (AI-12, D9).
 */
async function aiFlow(session: Session, env: NodeJS.ProcessEnv, app: string) {
  log("merge a branch that conflicts again");
  const appRow: Path = [...TREE, ["treeitem", "/^app/"]];
  await session.click(appRow);
  await session.until(
    "app alone selected",
    `const rows = [...document.querySelectorAll('[role="tree"] [role="treeitem"][aria-selected="true"]')];
     return rows.length === 1 && rows[0] === find(args[0]);`,
    appRow,
  );
  await session.click([
    ["toolbar", "Main"],
    ["button", "Merge"],
  ]);
  const dialog: Path = [["dialog", "Merge into feature in app"]];
  await session.type([...dialog, ["textbox", "Branch, remote branch or tag"]], "topic");
  await session.click([...dialog, ["option", "topic"]]);
  await session.click([...dialog, ["button", "Merge"]]);
  await session.element(BANNER);
  await session.click([...HISTORY, ["option", "/Uncommitted changes/"]]);
  const resolveWithAi: Path = [...CONFLICT, ["button", "Resolve with AI"]];
  await session.element(resolveWithAi);

  log("Settings, AI: add the mock provider and make it the default");
  await inSettings(session, async () => {
    await session.click([...SETTINGS, ["button", "Add provider…"]]);
    await session.select([...SETTINGS, ["combobox", "/^Kind/"]], "mock");
    // Lower case, as for the group: WebKitWebDriver typed "Mock" as "mock".
    await session.type([...SETTINGS, ["textbox", "Name"]], "mock");
    await session.click([...SETTINGS, ["button", "Save"]]);
    await session.click([...SETTINGS, ["button", "Set mock as default"]]);
    await session.until(
      "mock as the default",
      `return find(args[0])?.textContent.includes("mock (default)") ?? null;`,
      SETTINGS,
    );
  });

  log("AI features off: no Resolve with AI; on again");
  await inSettings(session, () => session.click([...SETTINGS, ["checkbox", "AI features"]]));
  await session.element([...CONFLICT, ["button", "Mark resolved"]]);
  await session.gone(resolveWithAi);
  await inSettings(session, () => session.click([...SETTINGS, ["checkbox", "AI features"]]));

  log("Resolve with AI; the consent dialog; Allow");
  await session.click(resolveWithAi);
  const consent: Path = [["alertdialog", "Send to mock?"]];
  await session.click([...consent, ["button", "Allow"]]);
  await session.gone(consent);

  log("the proposal shows; Accept all high-confidence");
  await session.click([...CONFLICT, ["button", "Accept all high-confidence"]]);
  await session.textIs('section[aria-label="Conflict"] .cm-content', ["feature and topic"]);
  // Accepting changed the buffer only (AI-12): git still has the conflict.
  if (!git(env, app, ["status", "--porcelain"]).includes("UU a.txt")) {
    throw new Error("a.txt left the conflict before Mark resolved");
  }

  log("Mark resolved, Continue");
  await session.click([...CONFLICT, ["button", "Mark resolved"]]);
  await session.until(
    "all conflicts resolved",
    `const banner = find(args[0]);
     return banner !== null && banner.textContent.includes("All conflicts are resolved and staged.");`,
    BANNER,
  );
  await session.click([...BANNER, ["button", "Continue"]]);
  await session.gone(BANNER);

  log("git says the merge is done, and the file holds the proposed text");
  await session.element([...HISTORY, ["option", "/Merge branch 'topic'/"]]);
  if (existsSync(join(app, ".git", "MERGE_HEAD"))) {
    throw new Error("the merge is still in progress");
  }
  const parents = git(env, app, ["rev-list", "--parents", "-n", "1", "HEAD"]).trim().split(" ").length - 1;
  if (parents !== 2) {
    throw new Error(`HEAD has ${parents} parents, not 2`);
  }
  const merged = readFileSync(join(app, "a.txt"), "utf8");
  if (merged !== "feature and topic\n") {
    throw new Error(`a.txt is ${JSON.stringify(merged)}`);
  }
}

async function main() {
  checkDrivers();
  for (const binary of [APP, CLI]) {
    if (!existsSync(binary)) {
      throw new Error(`no ${binary}; \`just smoke\` builds it`);
    }
  }

  // A throwaway home, config, cache and repos.
  const dir = mkdtempSync(join(tmpdir(), "gitcroc-smoke-"));
  const home = join(dir, "home");
  const config = join(dir, "config");
  mkdirSync(home);
  const env = {
    ...process.env,
    HOME: home,
    XDG_CONFIG_HOME: config,
    XDG_CACHE_HOME: join(dir, "cache"),
    GIT_CONFIG_NOSYSTEM: "1",
  };
  const repos = join(dir, "repos");
  mkdirSync(repos);
  const { lib, app } = fixture(repos, env);
  const replies = join(dir, "ai-replies.json");
  writeFileSync(replies, JSON.stringify([AI_REPLY]));
  execFileSync(CLI, ["workspace", "--config-dir", join(config, "gitcroc"), "add", lib], { env, stdio: "pipe" });
  log(`fixture in ${dir}`);

  // Add folder… picks the folder of both repos: the first workspace has lib
  // and gets app, the second gets both.
  // The mock provider kind answers from `replies` (M4-12).
  const driver = await startDriver({ ...env, GITCROC_E2E_PICK_FOLDER: repos, GITCROC_E2E_AI: replies });
  let failed = false;
  try {
    const session = await Session.start(APP);
    try {
      await smoke(session, app, env);
      await groupFlow(session, env, { lib, app });
      await aiFlow(session, env, app);
      log("smoke test passed");
    } catch (error) {
      failed = true;
      const path = join(ROOT, "target/e2e/failure.png");
      await session.screenshot(path).then(
        () => log(`the window at the failure: ${path}`),
        () => {},
      );
      throw error;
    } finally {
      await session.end().catch(() => {});
    }
  } finally {
    driver.kill();
    if (!failed) {
      rmSync(dir, { recursive: true, force: true });
    }
  }
}

main().catch((error: unknown) => {
  log(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
