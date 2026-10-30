# M6-16 Stage, unstage and discard; commit and amend; fsmonitor offer

## Goal
"Uncommitted changes" in the GPUI app becomes a place to work, as rc.1's
is. Files and hunks can be staged, unstaged and discarded, several files at
once. The commit box commits the staged changes or amends the last commit.
A large repo gets the fsmonitor offer once. These are the first jobs the
window starts itself, and they show in M6-15a's status bar and job list.

## Depends on
- M6-12: the changes pane and its Staged and Unstaged lists.
- M6-13a: the text diff, whose hunks get the controls.
- M6-15a: the status bar and job list the jobs show in.
- M6-05, PR 3: `changes_apply`, `changes_commit`, `fsmonitor_enable` and
  `fsmonitor_decline`.

## Spec
Implements: UI-15, PERF-7
Respects: GRP-8, GRP-9, OPS-7, ARCH-6, ARCH-7, ARCH-9, SEC-1, SEC-3, UI-22, UI-23, UI-29, D15, QA-3, QA-4

UI-15's lists and diff came with M6-12 and M6-13a. This task adds the rest
of it: the controls and the commit box. The core already decides what a
request does and refuses what it can't do (ARCH-6). The window only sends
requests and shows the answer.

## In scope
- **The changes model** (ARCH-6), the actions of `ui/src/stores/changes.ts`,
  per workspace (ARCH-9):
  - the files a control acts on: the selected file, or the files selected
    with it in the same list;
  - one `changes_apply` per action. A discard comes back as `confirm` with
    the repo and files to name, and runs only after the user confirms
    (GRP-9);
  - a hunk refused with `file_changed` shows a short message, and the view
    is read again;
  - the answer to the fsmonitor offer, per repo, for this run.
- **The file lists** (UI-15), the controls of `ui/src/changes/FileList.tsx`:
  - each Unstaged row has Discard and Stage, and each Staged row Unstage.
    Each list's header has Stage all or Unstage all;
  - Ctrl or Cmd and click adds or removes a file, and Shift and click
    selects the run from the selected file, within one list. A click in
    the other list starts a new selection. A row's control acts on the
    whole selection when the row is in it;
  - unmerged files get no controls (OPS-3, M6-19). Nor does a blocked repo
    (GRP-8): its controls are disabled, with the reason;
  - double-click on a row stages or unstages it, as in rc.1.
- **The hunk controls** (UI-15), as `ui/src/changes/DiffView.tsx` has them:
  - each hunk header of an uncommitted file's diff has Stage, or Unstage
    and Discard, in both inline and side-by-side mode;
  - no controls while Ignore whitespace is on, since those hunks don't
    apply to the file. The header says why;
  - F7 and Shift+F7 move to a hunk and give the focus to its first control
    (UI-22), as M6-13a left for this task.
- **The discard dialog** (GRP-9), a port of `ui/src/changes/DiscardDialog.tsx`,
  on the shared dialog parts:
  - it names the repo and the files the core named, as text (SEC-3), with
    up to 10 names and a count of the rest. For a hunk, it says the hunk's
    changes are lost;
  - Cancel has the focus, so Enter alone never discards. Escape cancels.
- **The commit box** (UI-15), a port of `ui/src/changes/CommitBox.tsx` and
  `ui/src/stores/commit.ts`, below the lists:
  - a subject field and a body field, then Commit and Amend;
  - Commit is disabled with nothing staged or no subject, and both are
    disabled in conflict state and while a commit of the repo runs. Each
    disabled button says why;
  - Ctrl or Cmd and Enter commits from either field;
  - Amend with an empty box fills it with the last commit's message, and a
    second Amend sends it. Amend with text sends it;
  - the unsent message is kept per repo of each workspace while the app
    runs, so it survives switching repos and tabs (UI-29). It is not saved;
  - when the commit succeeds, the box is cleared if it still holds what was
    sent. A refusal or a failure shows in the box, and the text stays.
- **The fsmonitor offer** (PERF-7), `FsmonitorOffer` in
  `ui/src/changes/ChangesPane.tsx`:
  - when the view's `offer_fsmonitor` names keys, a strip above the lists
    says which keys would be turned on and why, with "Turn on" and
    "Don't ask again";
  - "Turn on" runs `fsmonitor_enable` as a job, and "Don't ask again" calls
    `fsmonitor_decline`. Either way the strip goes. A refusal shows in it;
  - the app changes no git config without this answer.
- **Test seam:** `AppSetup::fsmonitor_threshold`, beside
  `commit_graph_threshold`, passed to the core's
  `Changes::with_fsmonitor_threshold`. The binary passes
  `FSMONITOR_THRESHOLD`, 50,000 files.
- **After a job:** the view is read again when the repo's summary changes
  (OPS-7), as M6-12 does. The selection is kept when its file is still
  listed, in either list.
- **Strings (D15):** the text of the controls, the discard dialog, the
  commit box and the offer moves from `ui/src/strings/en.ts` to
  `strings.rs`.

## Tests (QA-3)
Real git on `repo-gen` fixtures (QA-2). Jobs are waited for by their
`JobEnded`, never by time (no timing in CI).
- `ui_15_stage_and_unstage_files_from_the_lists`: the row controls, the
  list buttons and a double-click move files between Staged and Unstaged.
- `ui_15_several_files_go_in_one_job`: Ctrl and Shift selections, and one
  job for all of them.
- `ui_15_stage_unstage_and_discard_a_hunk`: in inline and side-by-side mode.
- `ui_15_a_changed_file_refuses_its_hunk`: the file changes after its diff
  was read, and the message shows.
- `ui_15_hunks_have_no_controls_while_ignoring_whitespace`.
- `grp_9_discard_asks_and_names_the_files`:
  - the dialog names the repo and the files, and Cancel has the focus;
  - Escape keeps the changes, and Discard drops them;
  - 12 files show 10 names and "and 2 more".
- `ui_15_commit_and_amend`:
  - Commit is disabled without a subject and with nothing staged;
  - Ctrl+Enter commits, and the box is cleared;
  - Amend with an empty box fills it with the last message, and the
    second Amend amends.
- `ui_15_commit_message_is_kept_per_repo`: two repos and two tabs, each
  with its own text.
- `ui_15_a_failed_commit_keeps_the_message`: a repo with no identity.
- `grp_8_blocked_repo_has_no_change_controls`.
- `perf_7_fsmonitor_offer_asks_once`:
  - a repo over the threshold shows the offer. The test lowers the
    threshold through `AppSetup::fsmonitor_threshold`;
  - "Turn on" sets the keys in the repo's config by a job;
  - "Don't ask again" hides it after a restart too;
  - a repo under the threshold has no offer.
- `ui_22_f7_reaches_a_hunk_control`.
- `sec_3_discard_dialog_renders_names_as_text`.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`
  and the keymap docs test keep passing over the new files.

## Out of scope
- **The group commit box and its sectioned lists** (UI-34): M6-22.
- **Unmerged files, Mark resolved and the conflict editor** (OPS-3 to
  OPS-9): M6-19.
- **The palette's Stage, Unstage, Discard, Commit and Amend** (UI-22):
  M6-24. The commands they would run are there for it.
- **AI writing the commit message:** not in the spec.
- **Announcing the job's outcome to screen readers:** M6-25.

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`:
  - stage and unstage a file and a hunk, discard a hunk, and commit;
  - amend that commit;
  - each job shows in the status bar and the job list.
- `just coverage M6` no longer lists UI-15 or PERF-7.
- `just check` passes.
