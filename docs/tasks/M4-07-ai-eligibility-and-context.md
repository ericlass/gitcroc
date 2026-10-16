# M4-07 Eligibility and the data sent, read from the repo

## Goal
For a file in conflict, the core decides whether it may go to AI and, if not, why. For an eligible file it reads exactly the data AI-5 allows: hunks with their context, side labels, and the commit messages of each side since the merge base.

## Depends on
M3 (it needs no other M4 task and can run beside M4-01 to M4-04)

## Spec
Implements: AI-4, AI-5
Respects: OPS-4, OPS-6, OPS-8, ARCH-1, ARCH-2, ARCH-5, SEC-3, ARCH-9, PLAT-2, section 7 "Where it appears" and "What is sent"

## In scope
- Types (`gitcroc-core::ai`), defined by the core because the core does not depend on `gitcroc-ai` (section 4 "Crates"). M4-08 maps them field by field onto M4-02's `FileRequest`.
  - `AiEligibility`: *eligible*, or *ineligible* with a reason: "not a text conflict" (OPS-6, with the kind), "over 1 MB", or "matches the exclude list (`<pattern>`)".
  - `AiFileContext`: the fields of AI-5, the same list as M4-02's `FileRequest`.
- Eligibility (AI-4):
  - A text conflict the editor shows (M2-09's rule) whose larger side is at most 1 MB, and whose path does not match a pattern of the exclude list.
  - Patterns are globs matched against the file name and against the path from the repo top, ignoring case on Windows and macOS. The list comes from the `[ai]` setting when M4-05 has landed, and the default list before that.
  - Choice conflicts are already ineligible through OPS-6. The 1 MB check still has its own test, since the two limits could drift apart.
- Context (AI-5), built from the conflict M2-09 reads (index stages, OPS-8), never from markers on disk:
  - Hunks are numbered from 1 in file order. Only the hunks the caller asks for are included, so a hunk the user already resolved in the editor (OPS-9) is not sent.
  - Each hunk has its base, ours and theirs. Its context is the 40 lines before and after it, taken from the merged text around it, which is resolved text and the ours side of neighbouring hunks. It is cut at the file's ends.
  - The labels are the ones the editor shows (OPS-4).
  - Commit messages: for each side, the commits that touched the file, reachable from that side's commit and not from the merge base of the two sides. Newest first, at most 10, each cut at 2,000 characters on a character boundary.
  - The sides' commits are those M2-09 labels: `HEAD` and `MERGE_HEAD`, `CHERRY_PICK_HEAD` or `REVERT_HEAD`. For a rebase, `HEAD` and the commit being applied; for a stash apply, `HEAD` and the stash commit.
  - A read through `gix`, with the CLI fallback of ARCH-2 behind the same trait. It is read-only, which the runner's read-only check confirms (ARCH-1).
- `conflict::chunks_from_texts(base, ours, theirs)` is exposed, the same merge M2-09 uses, so M4-11 can build hunks from fixture texts.
- `ai_eligibility` over IPC for a file of a changes view, by handle and id as M2-10 names files (PLAT-2), so the editor can show the reason (M4-09).
- `gitcroc-cli conflict ai-context <repo> <path> [--hunks 1,3]` prints the context as JSON, for tests and for debugging a prompt.
- Tests (fixture repos built in code, QA-2):
  - `ai_4_eligible_text_conflict`.
  - `ai_4_binary_and_delete_conflicts_are_ineligible_with_the_reason`.
  - `ai_4_over_1_mb_is_ineligible`.
  - `ai_4_default_exclude_list`, one file for each of the five patterns, and an edited list.
  - `ai_5_context_has_40_lines_each_side_cut_at_file_ends`.
  - `ai_5_only_the_requested_hunks`.
  - `ai_5_commit_messages_per_side_since_the_merge_base`: 12 commits on one side give the newest 10, and a commit that did not touch the file is left out.
  - `ai_5_messages_cut_at_2000_characters`, with a multi-byte character at the cut.
  - `ai_5_sides_for_merge_rebase_cherry_pick_revert_and_stash_apply`.

## Out of scope
- Calling a provider and keeping proposals (M4-08). Showing the reason (M4-09).

## Acceptance
- The tests above exist and pass on all three platforms, and `just check` passes.
