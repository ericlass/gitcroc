# M6-02 Spike: the conflict editor in GPUI

## Goal
We know whether gpui-component's code editor can carry the conflict editor (OPS-3 to OPS-9, AI-11, AI-12), or which fallback M6-19 builds instead. The answer comes with a throwaway program that shows it.

This is a spike in the sense of section 14. It carries M6's biggest component risk. It ends with a recommendation and numbers, not with production code.

## Depends on
None. It can run beside M6-01, M6-03 and M6-04.

## Spec
Respects: OPS-3, OPS-4, OPS-5, OPS-8, OPS-9, AI-11, AI-12, UI-24, UI-25, SEC-3, ARCH-6

## In scope
- **A standalone crate, `spikes/gpui-conflict/`**, built like `spikes/gpui-history/`:
  - its own empty `[workspace]`;
  - `gitcroc-git` and `gitcroc-core` by path;
  - `gpui-kit` at the version the history spike pins.
  - It is not part of the workspace, and nothing depends on it.
- **The buffer.** It is the one the Tauri UI builds (`ui/src/conflict/buffer.ts`):
  - the core's `ConflictChunk`s in order, from `gitcroc_git::chunks_from_texts` or from `conflict_get` on a fixture repo;
  - each hunk written between conflict markers named with the labels of OPS-4.
  - Hunks are tracked by position and never parsed back from the text (OPS-8).
- **Build it on gpui-component's multi-line code editor input**, and find out for each item below whether the editor supports it, supports it with a workaround, or can't:
  1. Free editing of the whole text, with IME and undo.
  2. Hunk ranges that follow edits elsewhere in the text, as anchors.
  3. A hunk changed by hand loses its buttons.
  4. Highlights over a hunk's ours and theirs lines, in the code theme's conflict colours (UI-24).
  5. Syntax highlighting of the file's language, from the code theme, through gpui-component's `syntect` backend.
  6. Block elements between lines, holding real buttons:
     - a header per side naming its label;
     - Take ours, Take theirs and Take both;
     - Show base;
     - under a hunk, an AI proposal with its explanation, confidence, Accept, Reject and Edit (AI-11).
  7. Taking a side replaces the hunk in one undo step. Undo brings the hunk and its buttons back.
  8. Keys that move to the next and previous hunk, and that take a side.
  9. Reading the final text back, for Mark resolved.
  10. The editor state survives being hidden and shown again (OPS-9). This means keeping the entity, not rebuilding it.
- **Performance** on a generated 10,000-line file with 50 hunks, at the reference display's 144 Hz:
  - Scroll it with the history spike's method: 100 px a frame, frame intervals, CPU per frame.
  - Measure typing latency in the middle of the file.
  - Measure the time from opening to the first paint.
- **If the editor fails items 2, 6 or 7, build and measure the fallback.** The fallback is a `list` of blocks:
  - each resolved run is one editable block, and each hunk is its own editable block with its buttons;
  - the final text is their concatenation.
  - Say what the fallback loses compared with one buffer, for example selecting across blocks or searching the whole file.
- **A results table** in `spikes/gpui-conflict/README.md`, with one row per item above: supported, workaround (which) or missing. Add the numbers and a screenshot of each variant in `results/`.
- **A recommendation for M6-19:** the editor, the fallback, or a small editor of our own, with an estimate in pull requests.
- **The spike is committed on its own branch, `spike/gpui-conflict`,** like the history spike, and is never merged. Its README says it is throwaway. The findings M6-19 needs are copied into the M6-02 sign-off notes in the task README.

## Out of scope
- Real jobs, Mark resolved and Continue: the buffer's text is printed instead.
- The choice view of OPS-6 (two versions), which needs no editor.

## Acceptance
- The README's table answers all ten items, measured on the reference machine, and the scroll numbers come from the scripted run.
- The recommendation names what M6-19 builds.
- The spike builds with `cargo build --release --manifest-path spikes/gpui-conflict/Cargo.toml`, and `just check` still passes, because nothing in the workspace changed.
