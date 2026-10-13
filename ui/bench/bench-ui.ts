// `just bench-ui` (PERF-8, M1-12): the section 9 budgets the core cannot
// measure alone, on the real app and `target-project`, Linux only. It drives
// a `perf-trace` build through tauri-driver and WebKitWebDriver, plain W3C
// WebDriver over HTTP, and reads what src/perf/trace.ts recorded. Prints a
// JSON array in the shape `just bench` prints. Run by hand before a release,
// never in CI: hosted runners have no fixed speed.
//
// Node runs this file directly (type stripping), so it uses no TypeScript
// syntax that needs compiling, and no packages.
import { type ChildProcess, execFileSync, spawn } from "node:child_process";
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

const ROOT = resolve(import.meta.dirname, "../..");
const SET = "target-project";
const DRIVER = "http://127.0.0.1:4444";

/** Section 9 budgets of the scenarios measured here; memory is in MB. */
const BUDGETS: Record<string, { budget: number; unit: "ms" | "MB" }> = {
  "cold-start": { budget: 1000, unit: "ms" },
  "select-largest-repo": { budget: 200, unit: "ms" },
  // 60 fps: no frame longer than 1000 / 60 ms, to the 0.1 ms rAF reports.
  "scroll-frame": { budget: 16.7, unit: "ms" },
  "scroll-blank": { budget: 100, unit: "ms" },
  "select-commit": { budget: 150, unit: "ms" },
  "switch-repo": { budget: 100, unit: "ms" },
  "input-latency": { budget: 50, unit: "ms" },
  "memory-idle": { budget: 400, unit: "MB" },
};

/** Scroll speed: 100 px a frame, about 250 rows a second at 60 fps. */
const SCROLL_PX_PER_FRAME = 100;
const SWITCHES = 5;
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
    if (args[i] === "--verbose") {
      options.verbose = true;
      continue;
    }
    const value = args[i + 1];
    if (value === undefined) {
      throw new Error(`usage: bench-ui [--runs <n>] [--verbose] [--start-only] [--direct] [--data <dir>] [--app <path>] [--cli <path>]`);
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

const log = (message: string) => process.stderr.write(`${message}\n`);

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

const ELEMENT = "element-6066-11e4-a52e-4f735466cecf";
type Element = Record<typeof ELEMENT, string>;

async function wd(method: string, path: string, body?: unknown): Promise<unknown> {
  const response = await fetch(`${DRIVER}${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(180_000),
  });
  const json = (await response.json()) as { value: { error?: string; message?: string } | unknown };
  const value = json.value as { error?: string; message?: string } | null;
  if (!response.ok || (value !== null && typeof value === "object" && "error" in value && value.error)) {
    throw new Error(`${method} ${path}: ${response.status} ${JSON.stringify(json.value)}`);
  }
  return json.value;
}

async function driverUp(): Promise<boolean> {
  try {
    await wd("GET", "/status");
    return true;
  } catch {
    return false;
  }
}

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
): Promise<{ ms: number; busy: boolean }> {
  log(`  ${scenario}`);
  await session.page(`p.arm(args[0], args[1], args[2] ?? p.firstRow() ?? "");`, scenario, condition, argument);
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

async function runOnce(
  options: Options,
  repos: Repo[],
  commits: { hash: string; index: number }[],
  tracePath: string,
): Promise<Trace & { memory: number }> {
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
      return { ...(JSON.parse(readFileSync(tracePath, "utf8")) as Trace), memory: 0 };
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
    return { ...trace, memory };
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

function startDriver(env: NodeJS.ProcessEnv): ChildProcess {
  const driver = spawn("tauri-driver", [], { env, stdio: ["ignore", "ignore", "inherit"] });
  driver.on("error", (error) => {
    log(`cannot start tauri-driver: ${error.message}; install it with \`cargo install tauri-driver --locked\``);
    process.exit(2);
  });
  return driver;
}

function which(program: string): boolean {
  try {
    execFileSync("which", [program], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

async function main() {
  if (process.platform !== "linux") {
    throw new Error("bench-ui runs on Linux only (WebKitWebDriver)");
  }
  const options = parseArgs(process.argv.slice(2));
  for (const [program, install] of [
    ["tauri-driver", "cargo install tauri-driver --locked"],
    ["WebKitWebDriver", "sudo apt install webkit2gtk-driver (Debian, Ubuntu)"],
  ] as const) {
    if (!which(program)) {
      throw new Error(`${program} is not on PATH; install it with \`${install}\``);
    }
  }
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
  const driver = startDriver(env);
  try {
    for (let wait = 0; !(await driverUp()); wait++) {
      if (wait > 100) {
        throw new Error("tauri-driver did not start");
      }
      await sleep(100);
    }
    const samples = new Map<string, number[]>();
    const steps = new Map<string, number[]>();
    const add = (scenario: string, value: number) => samples.set(scenario, [...(samples.get(scenario) ?? []), value]);
    // One warm-up run, as `just bench` does, then the timed runs.
    for (let run = 0; run <= options.runs; run++) {
      log(run === 0 ? "warm-up run" : `run ${run} of ${options.runs}`);
      const trace = options.direct
        ? { ...(await startDirect(options, env, tracePath)), memory: 0 }
        : await runOnce(options, repos, commits, tracePath);
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
