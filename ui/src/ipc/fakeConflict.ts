// The conflict editor's calls for the fake core (fake.ts): the answers of
// the core's `conflict_get`, `conflict_resolve` and `conflict_choose`,
// simplified. Test support only; the real rules are the core's.
import type { Conflict, ConflictStart } from "./bindings";

/** A text conflict with one hunk, between `main` and `feature`, for `path`. */
export function textConflict(path: string): Conflict {
  return {
    path,
    labels: { ours: "main", theirs: "feature" },
    content: {
      kind: "text",
      chunks: [
        { kind: "resolved", text: "first\n" },
        { kind: "hunk", ours: "ours\n", base: "base\n", theirs: "theirs\n" },
        { kind: "resolved", text: "last\n" },
      ],
    },
    edited_outside: false,
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
