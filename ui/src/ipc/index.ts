// The only way into the core. Components import from here, never from
// `@tauri-apps/api` or `./bindings` directly (enforced by eslint.config.js).
import type { IpcError } from "./bindings";
import type { Core } from "./core";
import { fake } from "./fake";
import { real } from "./core";

export type { AppInfo, GitStatus, IpcError, JobId, JobProgress, WorkspaceId } from "./bindings";

// A build-time constant, so production builds drop the fake.
export const core: Core = __FAKE_CORE__ ? fake : real;

/** A command failed in the core. */
export class IpcFailure extends Error {
  constructor(readonly error: IpcError) {
    super(error.message);
    this.name = "IpcFailure";
  }
}

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: IpcError };

async function unwrap<T>(result: Promise<Result<T>>): Promise<T> {
  const settled = await result;
  if (settled.status === "error") {
    throw new IpcFailure(settled.error);
  }
  return settled.data;
}

export const appInfo = () => unwrap(core.commands.appInfo());

export const gitStatus = () => unwrap(core.commands.gitStatus());

export const gitRecheck = () => unwrap(core.commands.gitRecheck());

export const debugEmitProgress = (workspaceId: string) => unwrap(core.commands.debugEmitProgress(workspaceId));
