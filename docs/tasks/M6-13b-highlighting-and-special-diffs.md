# M6-13b Highlighting and special diffs

## Goal
The diff is highlighted by the file's syntax in the code theme's colours,
and the diffs that are not text show what rc.1's do: a binary file's sizes,
an image before and after, a diff over the line limit until the user loads
it, an untracked folder and a submodule's commits.

This is the second of three parts of M6-13, after M6-13a's text diff.

## Depends on
- M6-13a: the text diff and its rows.

## Spec
Implements: UI-18
Respects: UI-17, UI-24, UI-25, UI-26, UI-29, ARCH-7, ARCH-8, PERF-1, SEC-2, SEC-3, D15, QA-3, QA-4

## In scope
- **Syntax highlighting** (UI-24), as `applyLanguage` in
  `ui/src/changes/codemirror.ts` picks a language:
  - the syntax comes from the file's name, with `syntect`'s default
    syntaxes; an unknown type stays plain text;
  - highlighting runs off the UI thread (ARCH-7). Each side's lines are
    parsed in order, so a line's colours follow from the lines above it
    in the same hunk, or in the whole file in side-by-side mode;
  - the colours are cached per line with the rows, so scrolling does no
    highlighting, and the rows show plain text until theirs arrive;
  - a change of code theme, or of light and dark, recolours the diff
    without reading it again;
  - added and removed backgrounds stay under the syntax colours, and keep
    4.5:1 against them (UI-25, `theme.rs`'s contrast tests).
- **The code font** (UI-26) from the appearance settings, as M6-07 gives it.
- **Special diffs** (UI-18), a port of `ui/src/changes/SpecialDiff.tsx`:
  - a binary file says "Binary file changed" with both sizes, in B, KB, MB
    or GB;
  - an image shows each side's format and size and the image itself,
    decoded off the UI thread from the bytes `diff_get` returns (in the
    same process they come with the diff, so `diff_image` is not needed)
    and drawn with `img()`, with no file or URL made (SEC-2). A side that
    does not exist says "none". Images are scaled down to fit, never up,
    and released from the GPU when the diff goes;
  - a diff over 20,000 changed lines shows its counts and **Load diff**,
    which reads it again with `load_anyway`. The choice holds for that file
    until another file is selected;
  - an untracked folder says its files are not read;
  - a submodule shows its commit before and after, in full.
- **Per tab** (UI-29): a loaded large diff and the images stay with their
  tab.
- **Strings (D15):** the text of these diffs moves to `strings.rs`.

## Tests (QA-3)
- `ui_18_binary_files_show_their_sizes`.
- `ui_18_images_show_before_and_after`: a changed PNG shows both sides
  with format and size; an added one says "none" before.
- `ui_18_a_large_diff_waits_for_load_diff`: over the limit shows the counts;
  Load diff shows the lines.
- `ui_18_folders_and_submodules`.
- `ui_24_the_diff_is_highlighted_by_its_syntax`: a Rust file's keyword has
  the theme's keyword colour; an unknown extension is plain; a theme change
  recolours it.
- `perf_1_highlighting_runs_off_the_ui_thread`: the rows show before their
  colours arrive.
- Unit tests: size formatting, the syntax by file name, highlighting state
  carried across lines.

## Out of scope
- **Docking** (UI-19): M6-13c.
- **The conflict editor's highlighting**: M6-19.
- **Releasing images and diffs of background tabs** (UI-31): M6-14.

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`: a Rust
  file's diff is highlighted in both modes; an image and a binary file show
  as above; the root commit's large file waits for Load diff.
- `just coverage M6` no longer lists UI-18.
- `just check` passes.
