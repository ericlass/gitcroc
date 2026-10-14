// Changes for the fake core (fake.ts): commit headers, changed files and
// file diffs, built like the core builds them from a few line operations.
// Test support only; the real lists and diffs come from the core.
import type {
  ChangeAction,
  ChangedFile,
  CommitDetails,
  CommitMessage,
  Context,
  DiffRequest,
  FileDiff,
  Hunk,
  Line,
  UnmergedFile,
} from "./bindings";

/** One line of a fake text file's change. */
export type FakeOp = { keep: string } | { add: string } | { remove: string };

/** A changed file of a fake list, with what its diff returns. */
export interface FakeFile {
  file: Omit<ChangedFile, "id">;
  /** A text change; `diff` is used when it is absent. */
  ops?: FakeOp[];
  diff?: FileDiff;
  /** Base64 bytes of each side of an image diff. */
  images?: { old?: string; new?: string };
  /** Answers "too large" (UI-18) unless the request loads it anyway. */
  tooLarge?: boolean;
}

/** A fake commit: its header and its files against each parent. */
export interface FakeCommit {
  details: CommitDetails;
  /** Files by parent index. */
  files: FakeFile[][];
}

export interface FakeWorkingCopy {
  staged: FakeFile[];
  unstaged: FakeFile[];
  unmerged: Omit<UnmergedFile, "id">[];
  /** The last commit's message; `null` on a branch with no commit. Default: `LAST_MESSAGE`. */
  lastMessage?: CommitMessage | null;
  /** In conflict state (OPS-1). */
  inConflict?: boolean;
}

/** The last commit's message of a fake working copy that names none. */
export const LAST_MESSAGE: CommitMessage = { subject: "Last commit", body: "Why it was made." };

/** A 1×1 PNG. */
export const PNG_1X1 =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

const counts = (ops: FakeOp[]) => ({
  added: ops.filter((op) => "add" in op).length,
  removed: ops.filter((op) => "remove" in op).length,
});

/** A changed file with the line counts of `ops`, as the core counts them. */
export function textFile(path: string, ops: FakeOp[], status: ChangedFile["status"] = "modified"): FakeFile {
  return {
    file: { path, old_path: null, status, mode_change: null, ...counts(ops) },
    ops,
  };
}

/** A file whose diff is `diff`, without line counts. */
export function otherFile(path: string, diff: FileDiff, images?: FakeFile["images"]): FakeFile {
  return {
    file: { path, old_path: null, status: "modified", mode_change: null, added: null, removed: null },
    diff,
    images,
  };
}

/** `count` unchanged lines named `line <n>`, from `from`. */
export const keep = (from: number, count: number): FakeOp[] =>
  Array.from({ length: count }, (_, index) => ({ keep: `line ${from + index}` }));

const CONTEXT: Record<Context, number> = { three: 3, ten: 10, whole_file: Number.MAX_SAFE_INTEGER };

/** The text diff of `ops`: hunks with the requested context, both texts for the whole file. */
export function textDiff(ops: FakeOp[], request: DiffRequest): FileDiff {
  let oldLine = 0;
  let newLine = 0;
  const lines: Line[] = ops.map((op) => {
    if ("keep" in op) {
      return { kind: "context", old_line: ++oldLine, new_line: ++newLine, text: op.keep };
    }
    if ("add" in op) {
      return { kind: "added", old_line: null, new_line: ++newLine, text: op.add };
    }
    return { kind: "removed", old_line: ++oldLine, new_line: null, text: op.remove };
  });
  const context = CONTEXT[request.context];
  const shown = lines.map(() => false);
  lines.forEach((line, index) => {
    if (line.kind !== "context") {
      for (let at = Math.max(0, index - context); at <= Math.min(lines.length - 1, index + context); at++) {
        shown[at] = true;
      }
    }
  });
  const hunks: Hunk[] = [];
  let current: Line[] = [];
  const close = () => {
    if (current.length === 0) {
      return;
    }
    const first = current[0];
    const olds = current.filter((line) => line.kind !== "added");
    const news = current.filter((line) => line.kind !== "removed");
    hunks.push({
      old_start: olds[0]?.old_line ?? (first?.new_line ?? 1) - 1,
      old_lines: olds.length,
      new_start: news[0]?.new_line ?? (first?.old_line ?? 1) - 1,
      new_lines: news.length,
      lines: current,
    });
    current = [];
  };
  lines.forEach((line, index) => {
    if (shown[index]) {
      current.push(line);
    } else {
      close();
    }
  });
  close();
  const whole = request.context === "whole_file" || request.full_text;
  const text = (kind: Line["kind"]) =>
    lines
      .filter((line) => line.kind === "context" || line.kind === kind)
      .map((line) => `${line.text}\n`)
      .join("");
  return {
    kind: "text",
    ...counts(ops),
    hunks,
    old_text: whole ? text("removed") : null,
    new_text: whole ? text("added") : null,
    old_missing_newline: false,
    new_missing_newline: false,
  };
}

/** The diff a fake file returns for `request`. Whitespace is not ignored. */
export function diffOf(file: FakeFile, request: DiffRequest): FileDiff {
  if (file.ops) {
    const diff = textDiff(file.ops, request);
    if (file.tooLarge && !request.load_anyway && diff.kind === "text") {
      return { kind: "too_large", added: diff.added, removed: diff.removed };
    }
    return diff;
  }
  return file.diff ?? { kind: "directory" };
}

/** The files a commit changes when the fake state has nothing for it. */
export function defaultFiles(): FakeFile[] {
  return [
    textFile("README.md", [...keep(1, 4), { remove: "line 5" }, { add: "line five" }, ...keep(6, 4)]),
    otherFile(
      "assets/logo.png",
      {
        kind: "image",
        old: { format: "png", size: 68 },
        new: { format: "png", size: 68 },
      },
      { old: PNG_1X1, new: PNG_1X1 },
    ),
    textFile("src/main.rs", [
      ...keep(1, 20),
      { add: "fn added() {}" },
      ...keep(21, 10),
      { remove: "line 31" },
      ...keep(32, 10),
    ]),
  ];
}

/** The header of the commit `hash` when the fake state has nothing for it. */
export function defaultDetails(hash: string, parents: string[]): CommitDetails {
  return {
    hash,
    author: { name: "Ada Lovelace", email: "ada@example.com", time: { seconds: 1_704_067_200, offset: 3600 } },
    committer: { name: "Grace Hopper", email: "grace@example.com", time: { seconds: 1_704_070_800, offset: 0 } },
    message: `Change ${hash.slice(0, 7)}\n\nA body with <b>markup</b> that stays text.\n`,
    parents,
  };
}

/** Numbers a fake list as the core does: by index, after sorting by path. */
export function numbered(files: FakeFile[]): FakeFile[] {
  return [...files].sort((a, b) => (a.file.path < b.file.path ? -1 : a.file.path > b.file.path ? 1 : 0));
}

/**
 * A working copy after `action` on `files` of its lists, as the core's job
 * leaves it: staging moves a file to the staged list, unstaging moves it
 * back, discarding drops it. A hunk of a file with more than one moves a
 * copy and leaves the file where it is.
 */
export function applyChange(
  working: FakeWorkingCopy,
  action: ChangeAction,
  files: FakeFile[],
  partial: boolean,
): FakeWorkingCopy {
  const without = (list: FakeFile[]) => (partial ? list : list.filter((file) => !files.includes(file)));
  const moved = (status: ChangedFile["status"], to: ChangedFile["status"]) =>
    files.map((file) => (file.file.status === status ? { ...file, file: { ...file.file, status: to } } : file));
  const merge = (list: FakeFile[], add: FakeFile[]) => [
    ...list.filter((file) => !add.some((other) => other.file.path === file.file.path)),
    ...add,
  ];
  switch (action) {
    case "stage":
      return { ...working, unstaged: without(working.unstaged), staged: merge(working.staged, moved("untracked", "added")) };
    case "unstage":
      return { ...working, staged: without(working.staged), unstaged: merge(working.unstaged, moved("added", "untracked")) };
    case "discard":
      return { ...working, unstaged: without(working.unstaged) };
  }
}
