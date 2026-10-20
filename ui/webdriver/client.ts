// Plain W3C WebDriver over loopback HTTP to tauri-driver and the native
// driver: Linux for `just bench-ui`, Linux and Windows for `just smoke`.
//
// Node runs this file directly (type stripping), so it uses no TypeScript
// syntax that needs compiling, and no packages.
import { type ChildProcess, execFileSync, spawn } from "node:child_process";
import { closeSync, openSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import { DEBUGGER, driverPrograms } from "./platform.ts";

export const DRIVER = "http://127.0.0.1:4444";

export const ELEMENT = "element-6066-11e4-a52e-4f735466cecf";
export type Element = Record<typeof ELEMENT, string>;

export const log = (message: string) => process.stderr.write(`${message}\n`);

export async function wd(method: string, path: string, body?: unknown): Promise<unknown> {
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

function which(program: string): boolean {
  try {
    execFileSync(process.platform === "win32" ? "where.exe" : "which", [program], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

/** Fails naming how to install tauri-driver or its native driver when one is missing. */
export function checkDrivers() {
  for (const [program, install] of driverPrograms(process.platform)) {
    if (!which(program)) {
      throw new Error(`${program} is not on PATH; install it with \`${install}\``);
    }
  }
}

/** Starts tauri-driver with `env` for the app, and waits until it answers. */
export async function startDriver(env: NodeJS.ProcessEnv): Promise<ChildProcess> {
  const [[program]] = driverPrograms(process.platform);
  const driver = spawn(program, [], { env, stdio: ["ignore", "ignore", "inherit"] });
  driver.on("error", (error) => {
    log(`cannot start tauri-driver: ${error.message}; install it with \`cargo install tauri-driver --locked\``);
    process.exit(2);
  });
  // Polls until it answers; the 60 s deadline only ends a driver that never starts.
  for (let wait = 0; !(await driverUp()); wait++) {
    if (wait > 600) {
      driver.kill();
      throw new Error("tauri-driver did not start");
    }
    await sleep(100);
  }
  return driver;
}

/** Windows attach mode: start the app first, then wait for its real WebView2 endpoint. */
export async function startWebview(app: string, env: NodeJS.ProcessEnv, logPath: string): Promise<ChildProcess> {
  const output = openSync(logPath, "w");
  const child = spawn(app, [], { env, stdio: ["ignore", output, output] });
  closeSync(output);
  let launchError: Error | undefined;
  child.on("error", (error) => { launchError = error; });
  try {
    const deadline = Date.now() + 60_000;
    for (;;) {
      if (launchError !== undefined) throw launchError;
      if (child.exitCode !== null || child.signalCode !== null) {
        throw new Error(`the app exited before WebView2 was ready; see ${logPath}`);
      }
      const ready = await fetch(`http://${DEBUGGER}/json/version`, { signal: AbortSignal.timeout(1000) })
        .then(async (response) => response.ok && !!((await response.json()) as { webSocketDebuggerUrl?: string })
          .webSocketDebuggerUrl)
        .catch(() => false);
      if (ready) return child;
      if (Date.now() >= deadline) throw new Error(`WebView2 did not start; see ${logPath}`);
      await sleep(100);
    }
  } catch (error) {
    stopProcess(child);
    throw error;
  }
}

/** Include native-driver and WebView2 children when stopping Windows processes. */
export function stopProcess(child: ChildProcess) {
  if (process.platform === "win32" && child.pid !== undefined) {
    try {
      execFileSync("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore" });
    } catch {
      // It may already have exited after the session ended.
    }
  } else {
    child.kill();
  }
}
