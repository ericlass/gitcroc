// The conflict editor's state (OPS-3, OPS-5, OPS-6, OPS-8, OPS-9), keyed by
// workspace, repo and path (ARCH-9, UI-29): each conflicted file as the core
// read it, whether its edited-outside question was answered, and its unsaved
// buffer, kept while the app runs so that switching files, repos and tabs
// and back restores it. Nothing is written before Mark resolved; the core
// writes then (ARCH-1). A repo's state is dropped when its conflict state
// ends, by Abort or in a terminal, and a file's when it is no longer
// unmerged. The core checks every request; this store sends them and keeps
// what came back. It also keeps each file's AI proposal as the core made it
// and what the user did with each proposed hunk in the view (AI-11): they
// live and are dropped with the buffer, and all of them go when AI features
// are turned off (AI-16). Accepting a proposal is an edit of the buffer, so
// nothing about it is sent (AI-12).
import type { EditorState, StateEffect, Text } from "@codemirror/state";
import { createEffect, on, untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";
import {
  type AiEligibility,
  aiEligibility,
  aiProposalGet,
  type AiRefused,
  aiResolve,
  type Conflict,
  type ConflictChoice,
  conflictChoose,
  conflictGet,
  conflictResolve,
  type ConflictStart,
  core,
  type FileProposal,
  jobCancel,
  type JobId,
  type ProviderId,
  type RepoId,
  type WorkspaceId,
} from "../ipc";
import type { AiStore } from "./ai";

/** What the editor starts from: the conflict's hunks, or the file as it is on disk (OPS-8). */
export type Start = "conflict" | "disk";

/** What the user did with a proposed hunk in the view. */
export interface HunkView {
  rejected: boolean;
  /** The text in its inline editor while open. */
  editing: string | null;
}

/** A file's AI state (AI-3, AI-11). */
export interface FileAi {
  /** Whether the file may go to AI (AI-4); `null` until read. */
  eligibility: AiEligibility | null;
  /** A request is on its way. */
  starting: boolean;
  /** The proposal job running for the file, with its provider's name. */
  job: { id: JobId; provider: string } | null;
  /** The core kept a result: read it while the file is shown. */
  ready: boolean;
  proposal: FileProposal | null;
  /** "AI failed", with the reason (AI-9). */
  failed: string | null;
  /** Why the last request was refused or did not go out. */
  refusal: string | null;
  /** By proposed hunk id, counted from 1. */
  hunks: Record<number, HunkView | undefined>;
}

const noAi = (): FileAi => ({
  eligibility: null,
  starting: false,
  job: null,
  ready: false,
  proposal: null,
  failed: null,
  refusal: null,
  hunks: {},
});

/** What resolving with AI needs of the app besides the core (AI-3, AI-6). */
export interface AiAsk {
  /** Asks for consent to the provider; `true` to go on. */
  askConsent: (provider: ProviderId) => Promise<boolean>;
  /** Opens the provider settings. */
  openSettings: () => void;
  /** The default provider's name. */
  providerName: () => string;
}

/** A refusal the button shows beside it, in words. */
export function refusalText(refused: AiRefused): string {
  switch (refused.kind) {
    case "ai_off":
      return "AI features are off.";
    case "ai_disabled":
      return "AI is off for this workspace.";
    case "no_provider":
      return "No AI provider is the default.";
    case "provider_unusable":
      return `The AI provider cannot be used: ${refused.reason}`;
    case "ineligible":
      return `Not sent to AI: ${refused.message}`;
    case "consent_needed":
      return "Not sent: the provider was not allowed.";
  }
}

/** The reason of a failed job's message, without the prefix the editor shows itself. */
const failure = (message: string | null) => (message ?? "the job failed").replace(/^AI failed: /, "");

export interface FileConflict {
  conflict: Conflict;
  /** Edited outside and not answered yet: keep the file or start over (OPS-8). `null` once answered. */
  start: Start | null;
  /** The buffer differs from what it started with. */
  unsaved: boolean;
  /** Mark resolved was refused: the first conflict marker line, from 1 (OPS-5). */
  markers: number | null;
  /** A resolution is on its way to the core. */
  sending: boolean;
  error: string | null;
  ai: FileAi;
}

interface WorkspaceConflicts {
  /** By repo, then by path. */
  files: Record<RepoId, Record<string, FileConflict | undefined> | undefined>;
  /** The file being read. */
  loading: { repo: RepoId; path: string } | null;
  /** Why the last read failed. */
  error: string | null;
}

/** A buffer kept while its editor is closed: the whole editor state, undo included, and where it was scrolled. */
export interface SavedEditor {
  state: EditorState;
  initial: Text;
  scroll: StateEffect<unknown> | null;
}

const empty = (): WorkspaceConflicts => ({ files: {}, loading: null, error: null });

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

const keyOf = (id: WorkspaceId, repo: RepoId, path: string) => `${id}\n${repo}\n${path}`;

/** A file of the shown changes view: what the core's conflict calls name it by. */
export interface ViewFile {
  repo: RepoId;
  /** The changes view's handle and the file's id in its unmerged list. */
  handle: number;
  id: number;
  path: string;
}

/**
 * `onResolvedElsewhere` is called with the workspace when the core says a
 * file is no longer in conflict, so that the changes view is read again.
 */
export function createConflictStore(onResolvedElsewhere: (id: WorkspaceId) => void) {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceConflicts> }>({ byWorkspace: {} });
  /** Editor states are CodeMirror's objects, kept out of the reactive store. */
  const editors = new Map<string, SavedEditor>();

  const workspace = (id: WorkspaceId): WorkspaceConflicts => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (conflicts: WorkspaceConflicts) => void) => {
    setState(
      produce((draft) => {
        change((draft.byWorkspace[id] ??= empty()));
      }),
    );
  };

  const updateFile = (id: WorkspaceId, repo: RepoId, path: string, change: (file: FileConflict) => void) =>
    update(id, (c) => {
      const file = c.files[repo]?.[path];
      if (file !== undefined) {
        change(file);
      }
    });

  const file = (id: WorkspaceId, repo: RepoId, path: string): FileConflict | undefined =>
    workspace(id).files[repo]?.[path];

  const loading = (id: WorkspaceId) => workspace(id).loading;
  const error = (id: WorkspaceId) => workspace(id).error;

  /** The file has unsaved edits, for the file list's mark. */
  const unsaved = (id: WorkspaceId, repo: RepoId, path: string) => file(id, repo, path)?.unsaved ?? false;

  /**
   * Reads `target` unless its state is kept; a kept buffer is shown as it
   * was left. A file resolved meanwhile reads the changes view again.
   */
  const open = async (id: WorkspaceId, target: ViewFile) => {
    if (untrack(() => file(id, target.repo, target.path)) !== undefined) {
      return;
    }
    update(id, (c) => {
      c.loading = { repo: target.repo, path: target.path };
      c.error = null;
    });
    const current = () => {
      const now = untrack(() => loading(id));
      return now?.repo === target.repo && now.path === target.path;
    };
    try {
      const read = await conflictGet(id, target.handle, target.id);
      if (!current()) {
        return;
      }
      update(id, (c) => {
        c.loading = null;
        if (read?.kind === "conflict") {
          const conflict = read.conflict;
          (c.files[target.repo] ??= {})[target.path] = {
            conflict,
            start: conflict.edited_outside && conflict.content.kind === "text" ? null : "conflict",
            unsaved: false,
            markers: null,
            sending: false,
            error: null,
            // A proposal Resolve all kept before the file was opened is read once it is shown (AI-13).
            ai: { ...noAi(), ready: true },
          };
        }
      });
      if (read?.kind === "already_resolved") {
        onResolvedElsewhere(id);
      }
    } catch (failure) {
      if (current()) {
        update(id, (c) => {
          c.loading = null;
          c.error = message(failure);
        });
      }
    }
  };

  /** Answers the edited-outside question (OPS-8). */
  const answer = (id: WorkspaceId, repo: RepoId, path: string, start: Start) =>
    updateFile(id, repo, path, (f) => (f.start = start));

  const editor = (id: WorkspaceId, repo: RepoId, path: string) => editors.get(keyOf(id, repo, path));

  /** Keeps the editor state of a file; called on every change and when the editor closes. */
  const keep = (id: WorkspaceId, repo: RepoId, path: string, saved: SavedEditor) => {
    // An editor closing because its file was dropped keeps nothing.
    if (untrack(() => file(id, repo, path)) === undefined) {
      return;
    }
    editors.set(keyOf(id, repo, path), saved);
    const changed = !saved.state.doc.eq(saved.initial);
    if (untrack(() => file(id, repo, path)?.unsaved) !== changed) {
      updateFile(id, repo, path, (f) => (f.unsaved = changed));
    }
  };

  /** What a resolution's answer does: a job, a refusal naming a marker line, or a stale view. */
  const settle = (id: WorkspaceId, target: ViewFile, start: ConflictStart | null) => {
    if (start === null) {
      // A newer view replaced the request's; it is on its way.
      updateFile(id, target.repo, target.path, (f) => (f.sending = false));
      return;
    }
    switch (start.kind) {
      case "started":
        // The job's end re-reads the repo, and the file leaves the list.
        updateFile(id, target.repo, target.path, (f) => (f.sending = false));
        break;
      case "markers_remain":
        updateFile(id, target.repo, target.path, (f) => {
          f.sending = false;
          f.markers = start.line;
        });
        break;
      case "already_resolved":
        forget(id, target.repo, target.path);
        onResolvedElsewhere(id);
        break;
    }
  };

  const send = async (id: WorkspaceId, target: ViewFile, request: () => Promise<ConflictStart | null>) => {
    updateFile(id, target.repo, target.path, (f) => {
      f.sending = true;
      f.markers = null;
      f.error = null;
    });
    try {
      settle(id, target, await request());
    } catch (failure) {
      updateFile(id, target.repo, target.path, (f) => {
        f.sending = false;
        f.error = message(failure);
      });
    }
  };

  /** Mark resolved with `text` (OPS-5); `override` resolves although marker lines remain. */
  const resolve = (id: WorkspaceId, target: ViewFile, text: string, override: boolean) =>
    send(id, target, () =>
      conflictResolve(id, { handle: target.handle, file: target.id, text, override_markers: override }),
    );

  /** Resolves with one side (OPS-6). */
  const choose = (id: WorkspaceId, target: ViewFile, side: ConflictChoice) =>
    send(id, target, () => conflictChoose(id, { handle: target.handle, file: target.id, side }));

  const forget = (id: WorkspaceId, repo: RepoId, path: string) => {
    editors.delete(keyOf(id, repo, path));
    update(id, (c) => {
      const files = c.files[repo];
      if (files !== undefined) {
        files[path] = undefined;
      }
    });
  };

  /** Drops every file of `repo`: its conflict state ended (OPS-9). */
  const drop = (id: WorkspaceId, repo: RepoId) => {
    for (const key of [...editors.keys()]) {
      if (key.startsWith(keyOf(id, repo, ""))) {
        editors.delete(key);
      }
    }
    update(id, (c) => {
      c.files[repo] = undefined;
      if (c.loading?.repo === repo) {
        c.loading = null;
      }
    });
  };

  /** Drops the files of `repo` that are no longer unmerged, after its lists were read. */
  const prune = (id: WorkspaceId, repo: RepoId, unmerged: readonly string[]) => {
    const files = untrack(() => workspace(id).files[repo]);
    for (const [path, kept] of Object.entries(files ?? {})) {
      if (kept !== undefined && !unmerged.includes(path)) {
        forget(id, repo, path);
      }
    }
  };

  /** The files of workspace `id` with unsaved edits, by repo and path, for closing its tab (UI-29). */
  const unsavedFiles = (id: WorkspaceId): { repo: RepoId; path: string }[] =>
    Object.entries(workspace(id).files).flatMap(([repo, files]) =>
      Object.entries(files ?? {}).flatMap(([path, kept]) => (kept?.unsaved ? [{ repo, path }] : [])),
    );

  /** Drops everything of workspace `id`: its tab was closed. */
  const forgetWorkspace = (id: WorkspaceId) => {
    for (const key of [...editors.keys()]) {
      if (key.startsWith(`${id}\n`)) {
        editors.delete(key);
      }
    }
    update(id, (c) => Object.assign(c, empty()));
  };

  const updateAi = (id: WorkspaceId, repo: RepoId, path: string, change: (ai: FileAi) => void) =>
    updateFile(id, repo, path, (f) => change(f.ai));

  /** Reads whether `target` may go to AI (AI-4); a failed read leaves the check to the core. */
  const readEligibility = async (id: WorkspaceId, target: ViewFile) => {
    try {
      const read = await aiEligibility(id, target.handle, target.id);
      if (read?.kind === "read") {
        updateAi(id, target.repo, target.path, (ai) => (ai.eligibility = read.eligibility));
      } else if (read?.kind === "already_resolved") {
        onResolvedElsewhere(id);
      }
    } catch {
      // `ai_resolve` checks again and says why.
    }
  };

  /** Proposal jobs that ended, in case one ends before `ai_resolve` has answered with its id. */
  const endedJobs = new Set<JobId>();

  /**
   * Resolve with AI for `hunks` of `target`, counted from 1 (AI-3): the job's
   * result arrives as `ai_proposal_ready`. Asks for consent and then again
   * when the core needs it (AI-6), and opens the provider settings when
   * there is no provider.
   */
  const resolveWithAi = async (id: WorkspaceId, target: ViewFile, hunks: number[], ask: AiAsk) => {
    const { repo, path } = target;
    const request = () => aiResolve(id, { handle: target.handle, file: target.id, hunks });
    updateAi(id, repo, path, (ai) => {
      ai.starting = true;
      ai.refusal = null;
    });
    try {
      let start = await request();
      if (start?.kind === "refused" && start.refused.kind === "consent_needed") {
        start = (await ask.askConsent(start.refused.consent.provider_id)) ? await request() : null;
      }
      updateAi(id, repo, path, (ai) => (ai.starting = false));
      if (start?.kind === "started" && !endedJobs.has(start.job_id)) {
        const job = { id: start.job_id, provider: ask.providerName() };
        updateAi(id, repo, path, (ai) => {
          ai.job = job;
          ai.failed = null;
        });
      } else if (start?.kind === "already_resolved") {
        forget(id, repo, path);
        onResolvedElsewhere(id);
      } else if (start?.kind === "refused") {
        const refused = start.refused;
        if (refused.kind === "no_provider") {
          ask.openSettings();
        } else if (refused.kind === "ineligible") {
          const eligibility = { kind: "ineligible" as const, reason: refused.reason, message: refused.message };
          updateAi(id, repo, path, (ai) => (ai.eligibility = eligibility));
        } else if (refused.kind !== "consent_needed") {
          updateAi(id, repo, path, (ai) => (ai.refusal = refusalText(refused)));
        }
      }
    } catch (failure) {
      updateAi(id, repo, path, (ai) => {
        ai.starting = false;
        ai.refusal = message(failure);
      });
    }
  };

  /** Cancels the file's proposal job; its end arrives as `job_ended`. */
  const cancelAi = (id: WorkspaceId, repo: RepoId, path: string) => {
    const job = untrack(() => file(id, repo, path)?.ai.job);
    if (job) {
      jobCancel(job.id).catch((failure: unknown) =>
        updateAi(id, repo, path, (ai) => (ai.refusal = message(failure))),
      );
    }
  };

  /** Reads the result the core kept for `target`; a stale view leaves it to the next one. */
  const readProposal = async (id: WorkspaceId, target: ViewFile) => {
    const { repo, path } = target;
    try {
      const read = await aiProposalGet(id, target.handle, target.id);
      if (read === null) {
        return;
      }
      switch (read.kind) {
        case "proposal":
        case "failed":
          updateAi(id, repo, path, (ai) => {
            ai.ready = false;
            ai.proposal = read.kind === "proposal" ? read.proposal : null;
            ai.failed = read.kind === "failed" ? read.reason : null;
            ai.hunks = {};
          });
          break;
        case "no_proposal":
          updateAi(id, repo, path, (ai) => (ai.ready = false));
          break;
        case "already_resolved":
          forget(id, repo, path);
          onResolvedElsewhere(id);
          break;
      }
    } catch (failure) {
      updateAi(id, repo, path, (ai) => {
        ai.ready = false;
        ai.refusal = `Cannot read the proposal: ${message(failure)}`;
      });
    }
  };

  /** Reject: the proposal leaves hunk `hunk` in the view. */
  const rejectHunk = (id: WorkspaceId, repo: RepoId, path: string, hunk: number) =>
    updateAi(id, repo, path, (ai) => (ai.hunks[hunk] = { rejected: true, editing: null }));

  /** Opens hunk `hunk`'s inline editor with `text`, keeps what is typed in it, or closes it with `null`. */
  const editHunk = (id: WorkspaceId, repo: RepoId, path: string, hunk: number, text: string | null) =>
    updateAi(id, repo, path, (ai) => (ai.hunks[hunk] = { rejected: false, editing: text }));

  /** Drops every file's AI state: AI features were turned off (AI-16). */
  const dropAi = () =>
    setState(
      produce((draft) => {
        for (const conflicts of Object.values(draft.byWorkspace)) {
          for (const files of Object.values(conflicts.files)) {
            for (const kept of Object.values(files ?? {})) {
              if (kept !== undefined) {
                kept.ai = noAi();
              }
            }
          }
        }
      }),
    );

  /** The files of workspace `id` whose proposal job is `jobId`. */
  const filesOfJob = (id: WorkspaceId, jobId: JobId) =>
    Object.entries(untrack(() => workspace(id).files)).flatMap(([repo, files]) =>
      Object.entries(files ?? {}).flatMap(([path, kept]) => (kept?.ai.job?.id === jobId ? [{ repo, path }] : [])),
    );

  /**
   * Drops a repo's state when a fresh read finds it out of conflict state
   * (OPS-9), and follows the proposal jobs: their end, and their results.
   */
  const subscribe = async (id: WorkspaceId) => {
    const stops = await Promise.all([
      core.events.repoSummary.listen((event) => {
        const { workspace_id, repo_id, refreshing, conflict, summary } = event.payload;
        if (workspace_id === id && !refreshing && summary !== null && !conflict) {
          drop(id, repo_id);
        }
      }),
      core.events.jobEnded.listen((event) => {
        const job = event.payload;
        if (job.workspace_id !== id) {
          return;
        }
        endedJobs.add(job.job_id);
        if (endedJobs.size > 100) {
          endedJobs.delete(endedJobs.values().next().value ?? job.job_id);
        }
        for (const { repo, path } of filesOfJob(id, job.job_id)) {
          updateAi(id, repo, path, (ai) => {
            ai.job = null;
            if (job.outcome?.kind === "failed") {
              ai.failed = failure(job.message);
            }
          });
        }
      }),
      core.events.aiProposalReady.listen((event) => {
        const { workspace_id, repo_id, path } = event.payload;
        if (workspace_id === id) {
          updateAi(id, repo_id, path, (ai) => {
            ai.ready = true;
            ai.job = null;
          });
        }
      }),
    ]);
    return () => stops.forEach((stop) => stop());
  };

  return {
    state,
    file,
    loading,
    error,
    unsaved,
    open,
    answer,
    editor,
    keep,
    resolve,
    choose,
    drop,
    prune,
    unsavedFiles,
    forgetWorkspace,
    subscribe,
    readEligibility,
    resolveWithAi,
    cancelAi,
    readProposal,
    rejectHunk,
    editHunk,
    dropAi,
  };
}

export type ConflictStore = ReturnType<typeof createConflictStore>;

/**
 * Drops every proposal, and the Resolve all runs that made them, when AI
 * features are turned off (AI-16); call it inside a component.
 */
export function followAiFeatures(ai: AiStore, conflicts: ConflictStore, runs?: { dropAll: () => void }) {
  createEffect(
    on(ai.enabled, (enabled, before) => {
      if (before === true && !enabled) {
        conflicts.dropAi();
        runs?.dropAll();
      }
    }),
  );
}
