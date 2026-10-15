// `just smoke` (section 12 "End to end smoke", M2-15): the real app, built
// with the `e2e` feature, on a fixture workspace through tauri-driver and
// WebKitWebDriver, Linux only. It adds a repo, stages a file, commits,
// merges a branch that conflicts, resolves the file in the conflict editor
// with Take theirs and an edit, marks it resolved and continues; the
// history then shows the merge commit and the repo is clean.
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
 * `notes.txt` to stage and commit, and `main` and `feature` that both
 * changed `a.txt`.
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
    button: 'button,[role="button"]',
    dialog: '[role="dialog"],dialog',
    listbox: '[role="listbox"]',
    menu: '[role="menu"]',
    menuitem: '[role="menuitem"]',
    option: '[role="option"]',
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

  /** Presses each chord of keys in turn, as a user types: all down, then all up. */
  async keys(chords: string[][]) {
    const actions = chords.flatMap((chord) => [
      ...chord.map((value) => ({ type: "keyDown", value })),
      ...[...chord].reverse().map((value) => ({ type: "keyUp", value })),
    ]);
    await wd("POST", `/session/${this.id}/actions`, { actions: [{ type: "key", id: "keyboard", actions }] });
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
  await session.click([["button", "Stage notes.txt"]]);
  await session.textIs(FILE_SECTIONS, ["Staged (1)", "Unstaged (0)"]);

  log("commit");
  await session.type(
    [
      ["region", "Commit message"],
      ["textbox", "Subject"],
    ],
    "Add notes",
  );
  await session.click([
    ["region", "Commit message"],
    ["button", "Commit"],
  ]);
  await session.element([...HISTORY, ["option", "/Add notes/"]]);

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
  execFileSync(CLI, ["workspace", "--config-dir", join(config, "gitcroc"), "add", lib], { env, stdio: "pipe" });
  log(`fixture in ${dir}`);

  const driver = await startDriver({ ...env, GITCROC_E2E_PICK_FOLDER: app });
  let failed = false;
  try {
    const session = await Session.start(APP);
    try {
      await smoke(session, app, env);
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
