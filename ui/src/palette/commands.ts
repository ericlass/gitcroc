// What each palette operation runs (UI-22). Each place of the window that
// has an operation's toolbar button, menu entry or pane control provides
// the palette with the same function that control calls, and the same rule
// that enables it: the palette adds no dialog, no way to run an operation
// and no rule of its own. A place provides only while it is shown, for
// the workspace it shows (ARCH-9); an operation that nothing provides now
// lacks its subject, and the palette names what to select.
import { createSignal } from "solid-js";
import type { PaletteOp, PaletteSubject, WorkspaceId } from "../ipc";
import { strings } from "../strings/en";

/** What an operation does now: run it, or why it cannot apply. */
export type Command = { run: () => void } | { reason: string };

/** An operation's command as its place sees it now; `null` when the selection does not give its subject. */
export type Provider = () => Command | null;

/** The places of the window an operation lives in. */
export type Place =
  /** The toolbar, the status bar, Settings and the tabs' menus, in `App`. */
  | "app"
  /** The tab bar's keys. */
  | "tabs"
  /** The tree's add menu and its rows' context menus. */
  | "tree"
  /** The history's commit menu. */
  | "history"
  /** The changes pane's file lists. */
  | "files"
  /** The changes pane's commit box. */
  | "commit"
  /** The conflict banner above the history. */
  | "banner"
  /** The conflict editor's toolbar and hunk buttons. */
  | "editor"
  /** The choice of a file that cannot be merged line by line (OPS-6). */
  | "choice";

/**
 * The place of every operation. A new operation in the core's catalog
 * fails here until it has one, and its place's provider until it is
 * provided.
 */
export const PLACES = {
  add_folder: "tree",
  clone: "tree",
  remove_repo: "tree",
  open_in_file_manager: "tree",
  open_in_terminal: "tree",
  stage_file: "files",
  unstage_file: "files",
  discard_file: "files",
  stage_all: "files",
  unstage_all: "files",
  commit: "commit",
  amend: "commit",
  checkout: "tree",
  create_branch: "app",
  rename_branch: "tree",
  delete_branch: "tree",
  set_upstream: "tree",
  checkout_remote_branch: "tree",
  checkout_commit: "history",
  fetch: "app",
  fetch_with_options: "app",
  pull: "app",
  pull_with_options: "app",
  push: "app",
  push_with_options: "app",
  merge: "app",
  rebase: "app",
  cherry_pick: "history",
  revert: "history",
  abort: "banner",
  continue: "banner",
  mark_resolved: "editor",
  take_ours: "editor",
  take_theirs: "editor",
  take_both: "editor",
  keep_ours: "choice",
  keep_theirs: "choice",
  stash: "app",
  stash_apply: "tree",
  stash_pop: "tree",
  stash_drop: "tree",
  create_tag: "tree",
  delete_tag: "tree",
  push_tag: "tree",
  reset_soft: "history",
  reset_mixed: "history",
  reset_hard: "history",
  add_remote: "tree",
  rename_remote: "tree",
  remove_remote: "tree",
  edit_remote_url: "tree",
  resolve_with_ai: "editor",
  resolve_all_with_ai: "banner",
  accept_high_confidence: "editor",
  settings: "app",
  new_workspace: "app",
  import_workspace: "app",
  export_workspace: "app",
  job_list: "app",
  close_tab: "app",
  next_tab: "tabs",
  previous_tab: "tabs",
  new_group: "tree",
} as const satisfies Record<PaletteOp, Place>;

/** The operations of `place`. */
export type OpsOf<P extends Place> = { [Op in PaletteOp]: (typeof PLACES)[Op] extends P ? Op : never }[PaletteOp];

/** What a place provides: a provider for each of its operations. */
export type Providers<P extends Place> = Record<OpsOf<P>, Provider>;

/** Runs `action`, or says it cannot apply: a control's `null` action is a disabled control. */
export const commandOf = (action: (() => void) | null, reason: string = strings.palette.cannotApply): Command =>
  action === null ? { reason } : { run: action };

/** The keys providers are kept by: a workspace's places, and the window's own. */
const keyOf = (workspace: WorkspaceId | null, place: Place) => `${workspace ?? ""}\n${place}`;

export function createCommandStore() {
  const [providers, setProviders] = createSignal<ReadonlyMap<string, Partial<Record<PaletteOp, Provider>>>>(new Map());

  /**
   * Provides `place`'s operations, for `workspace` or, with `null`, for
   * every workspace; until the returned function is called.
   */
  const provide = <P extends Place>(workspace: WorkspaceId | null, place: P, given: Providers<P>) => {
    const key = keyOf(workspace, place);
    const kept: Partial<Record<PaletteOp, Provider>> = given;
    setProviders((current) => new Map(current).set(key, kept));
    return () =>
      setProviders((current) => {
        if (current.get(key) !== kept) {
          return current;
        }
        const next = new Map(current);
        next.delete(key);
        return next;
      });
  };

  /** What `op` does now in `workspace`; `null` while its place is not shown or the selection lacks its subject. */
  const command = (workspace: WorkspaceId, op: PaletteOp): Command | null => {
    const place = PLACES[op];
    const provider = providers().get(keyOf(workspace, place))?.[op] ?? providers().get(keyOf(null, place))?.[op];
    return provider?.() ?? null;
  };

  return { provide, command };
}

export type CommandStore = ReturnType<typeof createCommandStore>;

/** Why an operation on `subject` cannot run while the selection does not give it one. */
export const missingSubject = (subject: PaletteSubject): string => strings.palette.select[subject];
