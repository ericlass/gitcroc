// The conflict editor's calls for the fake core (fake.ts): the answers of
// the core's `conflict_get`, `conflict_resolve` and `conflict_choose`,
// simplified. Test support only; the real rules are the core's.
import type { Conflict, ConflictDetails, ConflictLabels, ConflictStart } from "./bindings";

/**
 * The labels of the sides in a repo in conflict state `details`, as the
 * core names them (OPS-4): a rebase's ours is what it rebases onto.
 */
export function labelsOf(details: ConflictDetails | undefined): ConflictLabels {
  const onto = details?.onto ?? "main";
  const applying = details?.applying ?? "feature";
  switch (details?.operation) {
    case "revert":
      return { ours: onto, theirs: `revert of ${applying}` };
    case "apply_mailbox":
      return { ours: onto, theirs: "patch" };
    case undefined:
    case null:
      return { ours: onto, theirs: "stash" };
    default:
      return { ours: onto, theirs: applying };
  }
}

/** A text conflict with one hunk for `path`, labelled `labels`. */
export function textConflict(path: string, labels: ConflictLabels = { ours: "main", theirs: "feature" }): Conflict {
  return {
    path,
    labels,
    content: {
      kind: "text",
      chunks: [
        { kind: "resolved", text: "first\n" },
        { kind: "hunk", ours: "ours\n", base: "base\n", theirs: "theirs\n" },
        { kind: "resolved", text: "last\n" },
      ],
    },
    edited_outside: false,
    on_disk: null,
  };
}

const MARKER = /^([<|=>])\1{6}( |$)/;

/** The first conflict marker line of `text`, from 1, as the core finds it (OPS-5). */
export function firstMarkerLine(text: string): number | null {
  const index = text.split("\n").findIndex((line) => MARKER.test(line.replace(/\r$/, "")));
  return index < 0 ? null : index + 1;
}

/**
 * What the core answers a resolution of `path` before a job starts:
 * already resolved when the working copy no longer lists it, the marker
 * line Mark resolved refuses, or `null` to run.
 */
export function planResolution(
  unmerged: readonly string[],
  path: string,
  text: string | null,
  overrideMarkers: boolean,
): ConflictStart | null {
  if (!unmerged.includes(path)) {
    return { kind: "already_resolved" };
  }
  const line = text === null || overrideMarkers ? null : firstMarkerLine(text);
  return line === null ? null : { kind: "markers_remain", line };
}
