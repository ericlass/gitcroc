// The text a conflict editor starts with (OPS-3, OPS-8): the core's chunks
// in order, each hunk written between conflict markers named with the
// labels of OPS-4, so that a hunk left as it is keeps Mark resolved refused
// (OPS-5). Hunks are tracked by position; the parts and the markers are
// the core's text and labels, laid out here and never parsed back.
import type { ConflictChunk, ConflictLabels } from "../ipc";

/** A hunk of the buffer: where it is, what it was built from, and what it looks like untouched. */
export interface BufferHunk {
  /** Unique within its buffer. */
  id: number;
  /** Where the hunk is in the buffer, its marker lines included. */
  from: number;
  to: number;
  ours: string;
  base: string;
  theirs: string;
  /** The hunk's text as built. Changed, the hunk loses its buttons (OPS-3). */
  original: string;
  /** Lengths of its parts in `original`: the ours marker line, ours, the separator line, theirs. */
  layout: { head: number; ours: number; separator: number; theirs: number };
  /** Its base is shown under ours. */
  showBase: boolean;
}

export interface Buffer {
  text: string;
  hunks: BufferHunk[];
}

/** git's marker length (OPS-5). */
const MARKER = 7;

/** Line breaks as CodeMirror reads them; the core writes CRLF back where the file has it. */
export const normalize = (text: string) => text.replace(/\r\n?/g, "\n");

/** A label on one line. */
const oneLine = (label: string) => label.replace(/[\r\n]+/g, " ");

/** `text` ending in a line break, so a marker can follow it. */
const closed = (text: string) => (text === "" || text.endsWith("\n") ? text : `${text}\n`);

/** The buffer of a text conflict: resolved text as it is, hunks between markers. */
export function buildBuffer(chunks: ConflictChunk[], labels: ConflictLabels): Buffer {
  let text = "";
  const hunks: BufferHunk[] = [];
  for (const chunk of chunks) {
    if (chunk.kind === "resolved") {
      text += normalize(chunk.text);
      continue;
    }
    const [ours, base, theirs] = [normalize(chunk.ours), normalize(chunk.base), normalize(chunk.theirs)];
    const head = `${"<".repeat(MARKER)} ${oneLine(labels.ours)}\n`;
    const separator = `${"=".repeat(MARKER)}\n`;
    const end = `${">".repeat(MARKER)} ${oneLine(labels.theirs)}\n`;
    const original = head + closed(ours) + separator + closed(theirs) + end;
    hunks.push({
      id: hunks.length,
      from: text.length,
      to: text.length + original.length,
      ours,
      base,
      theirs,
      original,
      layout: { head: head.length, ours: closed(ours).length, separator: separator.length, theirs: closed(theirs).length },
      showBase: false,
    });
    text += original;
  }
  return { text, hunks };
}

export type Take = "ours" | "theirs" | "both";

/** What taking `side` of `hunk` puts in its place: both is ours, then theirs. */
export function takenText(hunk: BufferHunk, side: Take): string {
  switch (side) {
    case "ours":
      return hunk.ours;
    case "theirs":
      return hunk.theirs;
    case "both":
      return hunk.ours === "" ? hunk.theirs : closed(hunk.ours) + hunk.theirs;
  }
}

/** The hunk is as built: it keeps its buttons. */
export const untouched = (hunk: BufferHunk, slice: (from: number, to: number) => string) =>
  slice(hunk.from, hunk.to) === hunk.original;

/** Where the cursor goes for `hunk`: its first line of ours, or its start once changed. */
export function hunkTarget(hunk: BufferHunk, isUntouched: boolean): number {
  return isUntouched ? hunk.from + hunk.layout.head : hunk.from;
}

/** The hunk containing `position`, or `null`. */
export function hunkAt(hunks: readonly BufferHunk[], position: number): BufferHunk | null {
  return hunks.find((hunk) => hunk.from <= position && position < hunk.to) ?? null;
}

/** The hunk after (`1`) or before (`-1`) `position`, wrapping around; `null` when there is none. */
export function nextHunk(hunks: readonly BufferHunk[], position: number, direction: 1 | -1): BufferHunk | null {
  if (hunks.length === 0) {
    return null;
  }
  if (direction === 1) {
    return hunks.find((hunk) => hunk.from > position) ?? hunks[0] ?? null;
  }
  const current = hunkAt(hunks, position);
  const before = hunks.filter((hunk) => hunk.from < (current?.from ?? position + 1));
  return before.at(-1) ?? hunks.at(-1) ?? null;
}
