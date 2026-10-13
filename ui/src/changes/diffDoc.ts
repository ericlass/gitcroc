// The core's text diff in the shapes the diff view draws (UI-17): one
// document of the hunks' lines for the inline mode, and the changed ranges
// of both whole files for the side-by-side mode. Which lines changed is the
// core's answer, whitespace and context applied; nothing is diffed here.
import type { LineKind, TextDiff } from "../ipc";

/** The hunks of a text diff as one document, with what each line is. */
export interface InlineDoc {
  text: string;
  /** By document line, from 0. */
  kinds: LineKind[];
  oldNumbers: (number | null)[];
  newNumbers: (number | null)[];
  /** The document lines that start a hunk, with the hunk's header. */
  hunks: { line: number; header: string }[];
}

export function inlineDoc(diff: TextDiff): InlineDoc {
  const doc: InlineDoc = { text: "", kinds: [], oldNumbers: [], newNumbers: [], hunks: [] };
  const texts: string[] = [];
  for (const hunk of diff.hunks) {
    doc.hunks.push({
      line: texts.length,
      header: `@@ -${hunk.old_start},${hunk.old_lines} +${hunk.new_start},${hunk.new_lines} @@`,
    });
    for (const line of hunk.lines) {
      texts.push(line.text);
      doc.kinds.push(line.kind);
      doc.oldNumbers.push(line.old_line);
      doc.newNumbers.push(line.new_line);
    }
  }
  doc.text = texts.join("\n");
  return doc;
}

/** A changed range of the old (A) and new (B) text, in characters. */
export interface ChangeRange {
  fromA: number;
  toA: number;
  fromB: number;
  toB: number;
}

/** Where each line of `text` starts; the entry past the last line is its length. */
function lineStarts(text: string): number[] {
  const starts = [0];
  for (let at = text.indexOf("\n"); at >= 0 && at + 1 < text.length; at = text.indexOf("\n", at + 1)) {
    starts.push(at + 1);
  }
  starts.push(text.length);
  return starts;
}

/**
 * The changed ranges of the whole old and new text, one per run of removed
 * and added lines, each covering whole lines. `oldText` and `newText` are
 * the texts the diff came with.
 */
export function changeRanges(diff: TextDiff, oldText: string, newText: string): ChangeRange[] {
  const oldStarts = lineStarts(oldText);
  const newStarts = lineStarts(newText);
  // Line `n`, from 1, starts at `starts[n - 1]`; past the end is the length.
  const at = (starts: number[], line: number) => starts[Math.min(line - 1, starts.length - 1)] ?? 0;
  const ranges: ChangeRange[] = [];
  for (const hunk of diff.hunks) {
    // The last line seen on each side; a run starts after it.
    let lastOld = hunk.old_lines > 0 ? hunk.old_start - 1 : hunk.old_start;
    let lastNew = hunk.new_lines > 0 ? hunk.new_start - 1 : hunk.new_start;
    let run: { firstOld: number; firstNew: number; removed: number; added: number } | null = null;
    const close = () => {
      if (run !== null) {
        ranges.push({
          fromA: at(oldStarts, run.firstOld),
          toA: at(oldStarts, run.firstOld + run.removed),
          fromB: at(newStarts, run.firstNew),
          toB: at(newStarts, run.firstNew + run.added),
        });
        run = null;
      }
    };
    for (const line of hunk.lines) {
      if (line.kind === "context") {
        close();
      } else {
        run ??= { firstOld: lastOld + 1, firstNew: lastNew + 1, removed: 0, added: 0 };
        if (line.kind === "removed") {
          run.removed++;
        } else {
          run.added++;
        }
      }
      lastOld = line.old_line ?? lastOld;
      lastNew = line.new_line ?? lastNew;
    }
    close();
  }
  return ranges;
}
