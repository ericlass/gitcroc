// `just bench-ui` (PERF-8, M1-12): the section 9 budgets the core cannot
// measure alone, on the real app and `target-project`, Linux only. It drives
// a `perf-trace` build through tauri-driver and WebKitWebDriver (see
// ../webdriver/client.ts), and reads what src/perf/trace.ts recorded. Prints a
// JSON array in the shape `just bench` prints. Run by hand before a release,
// never in CI: hosted runners have no fixed speed.
//
// Node runs this file directly (type stripping), so it uses no TypeScript
// syntax that needs compiling, and no packages.
import { execFileSync, spawn } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  readlinkSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { checkDrivers, ELEMENT, type Element, log, startDriver, wd } from "../webdriver/client.ts";

const ROOT = resolve(import.meta.dirname, "../..");
const SET = "target-project";

/** Section 9 budgets of the scenarios measured here; memory is in MB. */
const BUDGETS: Record<string, { budget: number; unit: "ms" | "MB" }> = {
  "cold-start": { budget: 1000, unit: "ms" },
  "select-largest-repo": { budget: 200, unit: "ms" },
  // 60 fps: no frame longer than 1000 / 60 ms, to the 0.1 ms rAF reports.
  "scroll-frame": { budget: 16.7, unit: "ms" },
  "scroll-blank": { budget: 100, unit: "ms" },
  "select-commit": { budget: 150, unit: "ms" },
  "switch-repo": { budget: 100, unit: "ms" },
  // A group of all 20 repos: the first page of their merged history (PERF-5).
  "select-group": { budget: 300, unit: "ms" },
  "input-latency": { budget: 50, unit: "ms" },
  // The same row of section 9 with a real job running (M2-15).
  "input-latency-job": { budget: 50, unit: "ms" },
  "memory-idle": { budget: 400, unit: "MB" },
  // The same budget with a second tab of the same repos in the background,
  // after it was scrolled (UI-31).
  "memory-two-tabs": { budget: 400, unit: "MB" },
};

/** Scroll speed: 100 px a frame, about 250 rows a second at 60 fps. */
const SCROLL_PX_PER_FRAME = 100;
const SWITCHES = 5;
/** The group of every repo of the set, which `main` creates in its config copy. */
const ALL_GROUP = "All repos";
const KEYS = 20;
/** Changed lines a commit may have for select-commit (section 9). */
const SELECT_COMMIT_LINES = 5000;
/** The history's page size; the commit is picked from the first page. */
const PAGE_SIZE = 500;

interface Options {
  runs: number;
  /** Logs where cold start time and memory go. */
  verbose: boolean;
  /** Measures the cold start only, ending each run once the tree shows. */
  startOnly: boolean;
  /** With `startOnly`: starts the app itself, without WebDriver. */
  direct: boolean;
  /** Measures `input-latency-job` only. */
  jobOnly: boolean;
  data: string;
  app: string;
  cli: string;
}

function parseArgs(args: string[]): Options {
  const options: Options = {
    runs: 5,
    verbose: false,
    startOnly: false,
    direct: false,
    jobOnly: false,
    data: process.env.GITCROC_BENCH_DATA || join(ROOT, "target/bench-data"),
    app: join(ROOT, "target/bench-ui/release/gitcroc"),
    cli: join(ROOT, "target/release/gitcroc-cli"),
  };
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--start-only") {
      options.startOnly = true;
      continue;
    }
    if (args[i] === "--direct") {
      options.startOnly = true;
      options.direct = true;
      continue;
    }
    if (args[i] === "--job-only") {
      options.jobOnly = true;
      continue;
    }
    if (args[i] === "--verbose") {
      options.verbose = true;
      continue;
    }
    const value = args[i + 1];
    if (value === undefined) {
      throw new Error(`usage: bench-ui [--runs <n>] [--verbose] [--start-only] [--direct] [--job-only] [--data <dir>] [--app <path>] [--cli <path>]`);
    }
    switch (args[i]) {
      case "--runs":
        options.runs = Number(value);
        if (!Number.isInteger(options.runs) || options.runs < 1) {
          throw new Error("--runs takes a positive number");
        }
        break;
      case "--data":
        options.data = resolve(value);
        break;
      case "--app":
        options.app = resolve(value);
        break;
      case "--cli":
        options.cli = resolve(value);
        break;
      default:
        throw new Error(`unknown argument ${args[i]}`);
    }
    i++;
  }
  return options;
}

// --- The data set -----------------------------------------------------------

interface Repo {
  name: string;
  path: string;
  files: number;
}

function loadSet(data: string): Repo[] {
  const dir = join(data, SET);
  const manifest = join(dir, "repo-gen.json");
  if (!existsSync(manifest)) {
    throw new Error(`no ${SET} data in ${data}; run \`just bench-data ${SET}\` first`);
  }
  const json = JSON.parse(readFileSync(manifest, "utf8")) as { repos: { path: string; files?: number }[] };
  return json.repos
    .map((repo) => ({ name: basename(repo.path), path: join(dir, repo.path), files: repo.files ?? 0 }))
    .sort((a, b) => b.files - a.files);
}

/**
 * The commit of the largest repo's first history page with the most changed
 * lines under 5,000, as the core's `select-commit` picks it, and the next
 * one, so selections alternate. Counted by `git log --numstat` against the
 * first parent, in the history's order.
 */
function pickCommits(repo: string): { hash: string; index: number; lines: number }[] {
  const out = execFileSync(
    "git",
    [
      "-C",
      repo,
      "log",
      "--date-order",
      "--branches",
      "--remotes",
      "HEAD",
      `-n${PAGE_SIZE}`,
      "--diff-merges=first-parent",
      "--numstat",
      "--format=@%H",
    ],
    { encoding: "utf8", maxBuffer: 1 << 28 },
  );
  const commits: { hash: string; index: number; lines: number; files: number; counted: boolean }[] = [];
  for (const line of out.split("\n")) {
    if (line.startsWith("@")) {
      commits.push({ hash: line.slice(1), index: commits.length, lines: 0, files: 0, counted: true });
      continue;
    }
    const commit = commits.at(-1);
    const [added, removed] = line.split("\t");
    if (commit === undefined || removed === undefined) {
      continue;
    }
    commit.files++;
    if (added === "-" || removed === "-") {
      commit.counted = false;
    } else {
      commit.lines += Number(added) + Number(removed);
    }
  }
  // The core has no counts for a binary file or a list over 1,000 files.
  const best = commits
    .filter((commit) => commit.counted && commit.files > 0 && commit.files <= 1000)
    .filter((commit) => commit.lines < SELECT_COMMIT_LINES)
    .sort((a, b) => b.lines - a.lines)
    .slice(0, 2);
  if (best.length < 2) {
    throw new Error(`${repo} has no two commits under ${SELECT_COMMIT_LINES} changed lines`);
  }
  return best;
}

// --- WebDriver --------------------------------------------------------------

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
    await wd("POST", `/session/${session.id}/timeouts`, { script: 120_000 });
    return session;
  }

  /**
   * Runs `body` in the page as an async function, with the recorder as `p`
   * and the arguments as `args`, and returns its result.
   */
  async page<T>(body: string, ...args: unknown[]): Promise<T> {
    const script = `
      const done = arguments[arguments.length - 1];
      const args = Array.prototype.slice.call(arguments, 0, -1);
      (async () => {
        // The recorder loads just after the page; wait for it after a start.
        const since = Date.now();
        while (window.__gitcrocPerf === undefined) {
          if (Date.now() - since > 30000) throw new Error("this app has no perf-trace recorder");
          await new Promise((resolve) => setTimeout(resolve, 5));
        }
        const p = window.__gitcrocPerf;
        ${body}
      })().then(
        (value) => done({ ok: value === undefined ? null : value }),
        (error) => done({ error: String((error && error.message) || error) }),
      );`;
    const value = (await wd("POST", `/session/${this.id}/execute/async`, { script, args })) as {
      ok?: T;
      error?: string;
    };
    if (value.error !== undefined) {
      throw new Error(value.error);
    }
    return value.ok as T;
  }

  click(element: Element) {
    return wd("POST", `/session/${this.id}/element/${element[ELEMENT]}/click`, {});
  }

  type(element: Element, text: string) {
    return wd("POST", `/session/${this.id}/element/${element[ELEMENT]}/value`, { text });
  }

  find(css: string) {
    return wd("POST", `/session/${this.id}/element`, { using: "css selector", value: css }) as Promise<Element>;
  }

  /** Saves what the window shows as a PNG. */
  async screenshot(path: string) {
    const png = (await wd("GET", `/session/${this.id}/screenshot`)) as string;
    writeFileSync(path, Buffer.from(png, "base64"));
  }

  end() {
    return wd("DELETE", `/session/${this.id}`);
  }
}

/** Times one action: arms the recorder, acts, and returns the recorded result. */
async function timed(
  session: Session,
  scenario: string,
  condition: string,
  argument: string | null,
  act: () => Promise<unknown>,
  busy: "refresh" | "job" = "refresh",
): Promise<{ ms: number; busy: boolean }> {
  log(`  ${scenario}`);
  await session.page(
    `p.arm(args[0], args[1], args[2] ?? p.firstRow() ?? "", args[3]);`,
    scenario,
    condition,
    argument,
    busy,
  );
  await act();
  return session.page(`return p.result(args[0]);`, scenario);
}

// --- Memory -----------------------------------------------------------------

/** The app's process and all its descendants (the WebKit processes). */
function processTree(app: string): number[] {
  const target = realpathSync(app);
  const parents = new Map<number, number>();
  let root: number | null = null;
  for (const entry of readdirSync("/proc")) {
    const pid = Number(entry);
    if (!Number.isInteger(pid)) {
      continue;
    }
    try {
      const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
      const fields = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
      parents.set(pid, Number(fields[1]));
      if (root === null && readlinkSync(`/proc/${pid}/exe`) === target) {
        root = pid;
      }
    } catch {
      // Gone, or not ours.
    }
  }
  if (root === null) {
    throw new Error(`no running ${target}`);
  }
  const tree = [root];
  for (let i = 0; i < tree.length; i++) {
    for (const [pid, parent] of parents) {
      if (parent === tree[i]) {
        tree.push(pid);
      }
    }
  }
  return tree;
}

/** Proportional set size of each process in MB, so shared pages count once. */
function memoryMb(pids: number[]): { name: string; mb: number }[] {
  return pids.map((pid) => {
    const name = readFileSync(`/proc/${pid}/comm`, "utf8").trim();
    let kb: number;
    try {
      const rollup = readFileSync(`/proc/${pid}/smaps_rollup`, "utf8");
      kb = Number(/^Pss:\s+(\d+) kB/m.exec(rollup)?.[1] ?? 0);
    } catch {
      const status = readFileSync(`/proc/${pid}/status`, "utf8");
      kb = Number(/^VmRSS:\s+(\d+) kB/m.exec(status)?.[1] ?? 0);
    }
    return { name, mb: kb / 1024 };
  });
}

// --- One run of the app -----------------------------------------------------

interface Mark {
  name: string;
  unix_ms: number;
}

interface Trace {
  process_start_unix_ms: number | null;
  cold_start_ms: number | null;
  samples: { scenario: string; ms: number }[];
  /** When the kernel started the process, to 10 ms. */
  exec_unix_ms: number | null;
  /** Steps of the start the core saw, and those the page saw (M1-15). */
  core_marks: Mark[];
  page_marks: Mark[];
}

/** Each step of the start, in ms since `run()` started, earliest first. */
function startSteps(trace: Trace): [string, number][] {
  const start = trace.process_start_unix_ms;
  if (start === null) {
    return [];
  }
  const steps: [string, number][] = [["run() starts", 0]];
  if (trace.exec_unix_ms !== null) {
    steps.push(["process exec (to 10 ms)", trace.exec_unix_ms - start]);
  }
  for (const mark of trace.core_marks) {
    steps.push([`core: ${mark.name}`, mark.unix_ms - start]);
  }
  for (const mark of trace.page_marks) {
    steps.push([`page: ${mark.name}`, mark.unix_ms - start]);
  }
  return steps.sort((a, b) => a[1] - b[1]);
}

/**
 * Starts the app without WebDriver and returns its trace once the tree
 * shows: the cold start as a user gets it, with no automation session.
 */
async function startDirect(options: Options, env: NodeJS.ProcessEnv, tracePath: string): Promise<Trace> {
  rmSync(tracePath, { force: true });
  const app = spawn(options.app, [], { env, stdio: "ignore" });
  try {
    for (let wait = 0; ; wait++) {
      if (wait > 300) {
        throw new Error("the app wrote no trace within 30 s");
      }
      await sleep(100);
      try {
        return JSON.parse(readFileSync(tracePath, "utf8")) as Trace;
      } catch {
        // Not written yet, or only partly.
      }
    }
  } finally {
    app.kill();
    await new Promise((resolve) => app.once("exit", resolve));
  }
}

/** The workspace ids of the run: the one measured, and the second tab of `memory-two-tabs`. */
interface Tabs {
  main: string;
  second: string;
}

async function runOnce(
  options: Options,
  repos: Repo[],
  commits: { hash: string; index: number }[],
  tabs: Tabs,
  tracePath: string,
): Promise<Trace & { memory: number; twoTabs: number }> {
  const [largest, other] = repos;
  if (largest === undefined || other === undefined) {
    throw new Error(`${SET} has fewer than two repos`);
  }
  const session = await Session.start(options.app);
  try {
    await session.page(`await p.ready();`);
    const page = await session.page<{ origin: number; loaded: number }>(
      `const nav = performance.getEntriesByType("navigation")[0];
       return { origin: performance.timeOrigin, loaded: nav ? nav.domContentLoadedEventEnd : -1 };`,
    );

    if (options.startOnly) {
      rmSync(tracePath, { force: true });
      await session.page(`await p.flush();`);
      return { ...(JSON.parse(readFileSync(tracePath, "utf8")) as Trace), memory: 0, twoTabs: 0 };
    }

    // Tree rows are found again before each click: a refresh may re-render them.
    const repoRow = (repo: Repo) => session.page<Element>(`return p.repoRow(args[0]);`, repo.name);

    // Select the largest repo: first page of history.
    let row = await repoRow(largest);
    await timed(session, "select-largest-repo", "history", "", () => session.click(row));

    // Scroll the whole history at a fixed fast speed.
    const scrolled = await session.page<{ atEnd: boolean; rows: number }>(
      `return p.scroll(args[0], args[1]);`,
      SCROLL_PX_PER_FRAME,
      10_000,
    );
    if (!scrolled.atEnd) {
      log(`  scrolling stopped before the end (${scrolled.rows} rows)`);
    }

    // Select a commit under 5,000 changed lines, alternating between two.
    for (const commit of [commits[0], commits[1], commits[0]]) {
      if (commit === undefined) {
        continue;
      }
      row = await session.page<Element>(`return p.historyRow(args[0], args[1]);`, commit.index, commit.hash);
      await timed(session, "select-commit", "commit", commit.hash, () => session.click(row));
    }

    // Switch between two repos.
    for (let i = 0; i < SWITCHES; i++) {
      for (const repo of [other, largest]) {
        row = await repoRow(repo);
        await timed(session, "switch-repo", "history", null, () => session.click(row));
      }
    }

    // Select the group of all repos: the first page of their merged history,
    // then back to the largest repo, untimed.
    row = await session.page<Element>(`return p.groupRow(args[0]);`, ALL_GROUP);
    await timed(session, "select-group", "merged", "", () => session.click(row));
    row = await repoRow(largest);
    await session.click(row);
    await session.page(`
      while (document.querySelector('[role="listbox"][aria-label="History"] .history-repo') !== null) {
        await new Promise((resolve) => requestAnimationFrame(resolve));
      }`);

    // Input latency while a status refresh of every repo runs: the refresh
    // the window's focus starts (OPS-7), restarted whenever it ends.
    const filter = await session.find('input[aria-label="Filter repos and branches"]');
    await session.click(filter);
    let busy = 0;
    let value = "";
    for (let attempt = 0; busy < KEYS && attempt < KEYS * 4; attempt++) {
      await session.page(`
        if (document.querySelector(".tree-pane .spinner") === null) {
          window.dispatchEvent(new Event("focus"));
          const since = performance.now();
          while (document.querySelector(".tree-pane .spinner") === null && performance.now() - since < 2000) {
            await new Promise((resolve) => requestAnimationFrame(resolve));
          }
        }`);
      const next = value === "" ? "r" : "";
      const key = value === "" ? "r" : "";
      const result = await timed(session, "input-latency", "input", next, () => session.type(filter, key));
      value = next;
      if (result.busy) {
        busy++;
      }
    }
    if (busy < KEYS) {
      log(`  only ${busy} of ${KEYS} keys landed while a refresh ran`);
    }
    if (value !== "") {
      await session.type(filter, "");
    }

    // Memory once the workspace is idle.
    await session.page(`await p.idle(3000);`);
    const processes = memoryMb(processTree(options.app));
    const memory = processes.reduce((sum, process) => sum + process.mb, 0);
    if (options.verbose) {
      log(`  memory: ${processes.map((process) => `${process.name} ${process.mb.toFixed(0)} MB`).join(", ")}`);
    }
    const twoTabs = await memoryTwoTabs(session, options, largest, commits[0], tabs);

    rmSync(tracePath, { force: true });
    await session.page(`await p.flush();`);
    const trace = JSON.parse(readFileSync(tracePath, "utf8")) as Trace;
    if (options.verbose && trace.process_start_unix_ms !== null && trace.cold_start_ms !== null) {
      const toPage = page.origin - trace.process_start_unix_ms;
      log(
        `  cold start: ${toPage.toFixed(0)} ms to the page's start, ${page.loaded.toFixed(0)} ms to DOMContentLoaded, ` +
          `${(trace.cold_start_ms - toPage).toFixed(0)} ms from the page's start to the tree`,
      );
    }
    return { ...trace, memory, twoTabs };
  } catch (error) {
    const path = join(ROOT, "target/bench-ui/failure.png");
    await session.screenshot(path).then(
      () => log(`the window at the failure: ${path}`),
      () => {},
    );
    throw error;
  } finally {
    await session.end().catch(() => {});
  }
}

// --- Memory with a background tab (M3-03) ------------------------------------

/**
 * `memory-two-tabs`: opens the second workspace, of the same repos, as a
 * tab, scrolls its largest repo's whole history and selects a commit, then
 * goes back to the first tab and measures the memory once it is idle. The
 * second tab has let go of its history and diff by then (UI-31). Closes
 * the second tab, so the next run starts as this one did. Not timed: the
 * recorder's scroll samples stay those of the first tab.
 */
async function memoryTwoTabs(
  session: Session,
  options: Options,
  largest: Repo,
  commit: { hash: string; index: number } | undefined,
  tabs: Tabs,
): Promise<number> {
  log("  memory-two-tabs");
  const invoke = (command: string) =>
    session.page(`await window.__TAURI_INTERNALS__.invoke(args[0], { workspaceId: args[1] });`, command, tabs.second);
  await invoke("workspace_open");
  await session.page(
    `const opened = performance.now();
     while (p.repoRow(args[0]) === null) {
       if (performance.now() - opened > 30000) throw new Error("the second tab shows no " + args[0]);
       await new Promise((resolve) => requestAnimationFrame(resolve));
     }`,
    largest.name,
  );
  await session.click(await session.page<Element>(`return p.repoRow(args[0]);`, largest.name));
  // Down to the end of the history, a page at a time.
  await session.page(`
    const opened = performance.now();
    let list = null;
    while ((list = document.querySelector(".history-scroller")) === null || list.scrollHeight <= list.clientHeight) {
      if (performance.now() - opened > 30000) throw new Error("the second tab shows no history");
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    // Two seconds with no new page: the end, or a page that never comes.
    for (let quiet = 0; quiet < 120; ) {
      const height = list.scrollHeight;
      list.scrollTop = height;
      await new Promise((resolve) => requestAnimationFrame(resolve));
      quiet = list.scrollHeight === height ? quiet + 1 : 0;
    }`);
  if (commit !== undefined) {
    const row = await session.page<Element>(`return p.historyRow(args[0], args[1]);`, commit.index, commit.hash);
    await session.click(row);
  }
  await session.page(`await p.idle(1000);`);
  await session.page(
    `await window.__TAURI_INTERNALS__.invoke("workspace_activate", { workspaceId: args[0] });`,
    tabs.main,
  );
  await session.page(`await p.idle(3000);`);
  const processes = memoryMb(processTree(options.app));
  if (options.verbose) {
    log(`  memory: ${processes.map((process) => `${process.name} ${process.mb.toFixed(0)} MB`).join(", ")}`);
  }
  await invoke("workspace_close");
  return processes.reduce((sum, process) => sum + process.mb, 0);
}

// --- Input latency while a job runs (M2-15) ----------------------------------

/** git for the job scenario's own repos, with an identity for the commits it makes. */
function git(args: string[], env: NodeJS.ProcessEnv = process.env, input?: string): string {
  const identity = {
    GIT_AUTHOR_NAME: "Bench",
    GIT_AUTHOR_EMAIL: "bench@example.com",
    GIT_COMMITTER_NAME: "Bench",
    GIT_COMMITTER_EMAIL: "bench@example.com",
  };
  return execFileSync("git", args, { env: { ...env, ...identity }, encoding: "utf8", input }).trim();
}

/**
 * A local bare remote and a working copy of each repo, under `dir`. Both
 * borrow the data set's objects (`--shared`), so nothing of it is copied or
 * ever written: the other scenarios keep measuring the data as it was built.
 */
function jobRepos(repos: Repo[], dir: string): { name: string; remote: string; work: string }[] {
  return repos.map((repo) => {
    const remote = join(dir, "remotes", `${repo.name}.git`);
    const work = join(dir, "work", repo.name);
    git(["clone", "--quiet", "--bare", "--shared", repo.path, remote]);
    git(["clone", "--quiet", "--shared", remote, work]);
    return { name: repo.name, remote, work };
  });
}

/** Adds commit `wave` to the current branch of the bare `remote`, so a fetch has something to bring. */
function advance(remote: string, wave: number) {
  const env = { ...process.env, GIT_DIR: remote, GIT_INDEX_FILE: join(remote, "bench-index") };
  const branch = git(["symbolic-ref", "HEAD"], env);
  const head = git(["rev-parse", "HEAD"], env);
  git(["read-tree", head], env);
  const blob = git(["hash-object", "-w", "--stdin"], env, `wave ${wave}\n`);
  git(["update-index", "--add", "--cacheinfo", `100644,${blob},bench-job.txt`], env);
  const tree = git(["write-tree"], env);
  const commit = git(["commit-tree", tree, "-p", head, "-m", `Wave ${wave}`], env);
  git(["update-ref", branch, commit, head], env);
}

/**
 * `input-latency-job`: the filter key of `input-latency`, while a job
 * runs: a fetch of every repo from its local bare remote, then a pull of the
 * largest, again and again; each wave's remotes have a new commit. Keys
 * that land while no job runs count as `input-latency-job-idle`. Its own
 * app session, on a workspace of the working copies.
 */
async function jobLatency(options: Options, repos: Repo[], home: string, tracePath: string): Promise<number[]> {
  const dir = join(home, "job");
  log(`setting up a bare remote and a working copy of ${repos.length} repos in ${dir}`);
  const copies = jobRepos(repos, dir);
  const config = join(dir, "config");
  for (const copy of copies) {
    execFileSync(options.cli, ["workspace", "--config-dir", join(config, "gitcroc"), "add", copy.work], {
      stdio: "pipe",
    });
  }
  const shown = JSON.parse(
    execFileSync(options.cli, ["workspace", "--config-dir", join(config, "gitcroc"), "show"], { encoding: "utf8" }),
  ) as { id: string; repos: { id: string; name: string }[] };
  const ids = shown.repos.map((repo) => repo.id);
  const largest = shown.repos.find((repo) => repo.name === repos[0]?.name)?.id;
  if (largest === undefined) {
    throw new Error("the job scenario's workspace has no largest repo");
  }

  const env = {
    ...process.env,
    XDG_CONFIG_HOME: config,
    XDG_CACHE_HOME: join(dir, "cache"),
    GITCROC_PERF_TRACE: tracePath,
  };
  const driver = await startDriver(env);
  try {
    const session = await Session.start(options.app);
    try {
      await session.page(`await p.ready();`);
      await session.page(`await p.idle(1000);`);
      log("input-latency-job");
      const sync = (repo: string, op: unknown) =>
        `window.__TAURI_INTERNALS__.invoke("op_sync", {
           workspaceId: args[0],
           request: { repo_id: ${repo}, op: ${JSON.stringify(op)}, confirmed: false },
         })`;
      const filter = await session.find('input[aria-label="Filter repos and branches"]');
      await session.click(filter);
      const target = KEYS * options.runs;
      let busy = 0;
      let value = "";
      let wave = 0;
      let pullNext = false;
      for (let attempt = 0; busy < target && attempt < target * 6; attempt++) {
        const running = await session.page<boolean>(`return document.querySelector(".status-bar .job-running") !== null;`);
        if (!running) {
          if (pullNext) {
            await session.page(`await ${sync("args[1]", { kind: "pull", strategy: "config" })};`, shown.id, largest);
          } else {
            wave++;
            for (const copy of copies) {
              advance(copy.remote, wave);
            }
            await session.page(
              `for (const repo of args[1]) await ${sync("repo", { kind: "fetch", prune: "config" })};`,
              shown.id,
              ids,
            );
          }
          pullNext = !pullNext;
          // Until the status bar shows the job, so the next key lands while it runs.
          await session.page(`
            const asked = performance.now();
            while (document.querySelector(".status-bar .job-running") === null && performance.now() - asked < 2000) {
              await new Promise((resolve) => requestAnimationFrame(resolve));
            }`);
        }
        const next = value === "" ? "r" : "";
        const key = value === "" ? "r" : "\uE003";
        const result = await timed(
          session,
          "input-latency-job",
          "input",
          next,
          () => session.type(filter, key),
          "job",
        );
        value = next;
        if (result.busy) {
          busy++;
        }
      }
      log(`  ${busy} keys landed while a job ran, over ${wave} waves`);
      // The jobs did their work: the largest repo was pulled, and the others fetched.
      const pulled = git(["-C", copies[0]?.work ?? "", "log", "-1", "--format=%s"]);
      const fetched = git(["-C", copies[1]?.work ?? "", "log", "-1", "--format=%s", "origin/HEAD"]);
      if (!pulled.startsWith("Wave ") || !fetched.startsWith("Wave ")) {
        throw new Error(`the jobs did not bring the waves: pulled ${pulled}, fetched ${fetched}`);
      }
      log(`  ${repos[0]?.name} is at ${pulled}; ${repos[1]?.name} fetched ${fetched}`);
      if (busy < target) {
        log(`  only ${busy} of ${target} keys landed while a job ran`);
      }
      if (value !== "") {
        await session.type(filter, "\uE003");
      }
      rmSync(tracePath, { force: true });
      await session.page(`await p.idle(1000);`);
      await session.page(`await p.flush();`);
      const trace = JSON.parse(readFileSync(tracePath, "utf8")) as Trace;
      return trace.samples.filter((sample) => sample.scenario === "input-latency-job").map((sample) => sample.ms);
    } finally {
      await session.end().catch(() => {});
    }
  } finally {
    driver.kill();
  }
}

// --- Results ----------------------------------------------------------------

/** Median and nearest-rank 95th percentile, as `gitcroc-cli bench` computes them. */
function summarize(samples: number[]): [number, number] {
  const sorted = [...samples].sort((a, b) => a - b);
  const at = (index: number) => sorted[index] ?? Number.NaN;
  const n = sorted.length;
  const median = n % 2 === 1 ? at((n - 1) / 2) : (at(n / 2 - 1) + at(n / 2)) / 2;
  const rank = Math.max(1, Math.ceil((n * 95) / 100));
  return [median, at(rank - 1)];
}

function results(runs: number, samples: Map<string, number[]>) {
  const round = (value: number) => Math.round(value * 1000) / 1000;
  return [...samples].map(([scenario, values]) => {
    const [median, p95] = summarize(values);
    const budget = BUDGETS[scenario];
    return {
      scenario,
      data_set: SET,
      runs,
      samples: values.length,
      unit: budget?.unit ?? "ms",
      median_ms: round(median),
      p95_ms: round(p95),
      budget_ms: budget?.budget ?? null,
      pass: budget === undefined || median <= budget.budget,
    };
  });
}

async function main() {
  checkDrivers();
  const options = parseArgs(process.argv.slice(2));
  if (!existsSync(options.app)) {
    throw new Error(`no perf-trace app at ${options.app}; \`just bench-ui\` builds it`);
  }
  const repos = loadSet(options.data);
  const largest = repos[0];
  if (largest === undefined) {
    throw new Error(`${SET} has no repos`);
  }

  // A throwaway config and cache: a workspace of every repo, its summaries cached.
  const home = mkdtempSync(join(tmpdir(), "gitcroc-bench-ui-"));
  const config = join(home, "config");
  const cache = join(home, "cache");
  const tracePath = join(home, "trace.json");
  log(`setting up a workspace of ${repos.length} repos in ${home}`);
  for (const repo of repos) {
    execFileSync(options.cli, ["workspace", "--config-dir", join(config, "gitcroc"), "add", repo.path], {
      stdio: "pipe",
    });
  }
  // A second workspace of the same repos, closed, for `memory-two-tabs`.
  const workspace = (...args: string[]) =>
    execFileSync(options.cli, ["workspace", "--config-dir", join(config, "gitcroc"), ...args], { encoding: "utf8" });
  const main = (JSON.parse(workspace("show")) as { id: string }).id;
  const second = (JSON.parse(workspace("create", "Second tab")) as { active: string }).active;
  for (const repo of repos) {
    workspace("add", repo.path);
  }
  workspace("close", second);
  workspace("open", main);
  // target-project has no group of all its repos; select-group needs one.
  const ids = (JSON.parse(workspace("show")) as { repos: { id: string }[] }).repos.map((repo) => repo.id);
  execFileSync(options.cli, ["group", "--config-dir", join(config, "gitcroc"), "create", ALL_GROUP, ...ids], {
    stdio: "pipe",
  });
  const tabs = { main, second };
  execFileSync(
    options.cli,
    ["summaries", "--config-dir", join(config, "gitcroc"), "--cache-dir", join(cache, "gitcroc")],
    { stdio: "pipe" },
  );
  const commits = pickCommits(largest.path);
  log(`select-commit: ${commits.map((commit) => `${commit.hash.slice(0, 10)} (${commit.lines} lines)`).join(", ")}`);

  const env = {
    ...process.env,
    XDG_CONFIG_HOME: config,
    XDG_CACHE_HOME: cache,
    GITCROC_PERF_TRACE: tracePath,
  };
  const driver = await startDriver(env);
  try {
    const samples = new Map<string, number[]>();
    const steps = new Map<string, number[]>();
    const add = (scenario: string, value: number) => samples.set(scenario, [...(samples.get(scenario) ?? []), value]);
    // One warm-up run, as `just bench` does, then the timed runs.
    for (let run = 0; run <= (options.jobOnly ? -1 : options.runs); run++) {
      log(run === 0 ? "warm-up run" : `run ${run} of ${options.runs}`);
      const trace = options.direct
        ? { ...(await startDirect(options, env, tracePath)), memory: 0, twoTabs: 0 }
        : await runOnce(options, repos, commits, tabs, tracePath);
      if (run === 0) {
        continue;
      }
      if (trace.cold_start_ms === null) {
        throw new Error("the tree never showed the cached repos");
      }
      add("cold-start", trace.cold_start_ms);
      for (const [name, ms] of startSteps(trace)) {
        steps.set(name, [...(steps.get(name) ?? []), ms]);
      }
      if (options.startOnly) {
        continue;
      }
      for (const sample of trace.samples) {
        add(sample.scenario, sample.ms);
      }
      add("memory-idle", trace.memory);
      add("memory-two-tabs", trace.twoTabs);
    }
    if (options.verbose) {
      log("the start, median and range in ms since run() started:");
      const rows = [...steps].map(([name, values]) => ({
        name,
        median: summarize(values)[0],
        min: Math.min(...values),
        max: Math.max(...values),
      }));
      for (const row of rows.sort((a, b) => a.median - b.median)) {
        log(
          `  ${row.median.toFixed(0).padStart(6)}  (${row.min.toFixed(0)} to ${row.max.toFixed(0)})  ${row.name}`,
        );
      }
    }
    if (!options.startOnly) {
      // Its own session and workspace, so it ends this driver first.
      driver.kill();
      for (const ms of await jobLatency(options, repos, home, tracePath)) {
        add("input-latency-job", ms);
      }
    }
    const report = results(options.runs, samples);
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    process.exitCode = report.every((result) => result.pass) ? 0 : 1;
  } finally {
    driver.kill();
    rmSync(home, { recursive: true, force: true });
  }
}

main().catch((error: unknown) => {
  log(error instanceof Error ? error.message : String(error));
  process.exitCode = 2;
});
