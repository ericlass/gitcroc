# M6-25a The accessibility tree test

## Goal
A screen reader finds a role and a name on every interactive element of the
M5-10 views, the selection and position in every list and the tree, a name
and text on every dialog, and one live status that announces failed and
conflicted jobs. A Linux CI job proves it from the AccessKit tree of a real
window, as M6-04 did in its spike (README decisions table, "Accessibility
bar in M6", musts 1, 2, 4 and 5).

## Depends on
- M6-04: the spike's tree dump, its checker and its AT-SPI probe, on
  `spike/gpui-a11y` at `e99c9bb`. They are ported, not merged.
- M6-24b: every view this task dumps is reachable from the keyboard.

## Spec
Respects: UI-3, UI-6, UI-21, UI-22, UI-35, D15, ARCH-6, QA-2, QA-3, QA-4, section 12 "Front end flows" row

## In scope
- **The harness:**
  - **Test binary:** `crates/gitcroc-ui/tests/a11y_tree/`, with
    `harness = false`. It opens a real window, not the test platform,
    because `#[gpui::test]` never activates the AccessKit tree. It runs the
    real core on `repo-gen` fixtures (QA-2) and reaches each view by
    dispatching the app's actions.
  - **Output:** in each view it waits for the tree to activate and writes
    `Window::debug_a11y_tree_json()`.
  - **Session:** a port of the spike's `a11y-session.sh` (Xvfb, a D-Bus
    session and an enabled AT-SPI bus).
  - **Running it:** `just a11y` runs it. `just check` runs it on Linux when
    `xvfb-run` is found, and otherwise prints that it skipped.
  - **CI:** a step in the Linux CI job runs it, after "presents its first
    frame", adding `at-spi2-core` and `dbus` to the installed packages. It
    has a timeout.
- **The views,** M5-10's list:
  - the main window with a repo, a group, and the uncommitted changes
    selected;
  - the conflict editor with an AI proposal;
  - Settings with every tab it has (the AI tab until M6-26, which adds its
    tabs to this test);
  - the job list and a result report;
  - the preflight dialog;
  - a destructive confirmation;
  - the command palette;
  - the git setup screen;
  - the empty state with no tab open.
- **The rules,** checked over each dump by a checker with its own unit
  tests, which need no display:
  - **Names and roles:** every interactive node (by role, or because it
    takes a click, the focus or a value) has a role and a non-empty name.
    The window buttons are named and not in the Tab order (UI-35).
  - **Lists:** the history, file lists, palette and job list are
    `ListBox`es, with the selected row exposed and the focus reported on it
    (active descendant). A row's name or description carries its position,
    such as "43 of 1,000", because only visible rows reach the tree (M6-04
    finding 6). AccessKit's `aria_position_in_set` is 0-based (finding 2).
  - **The repo tree:** `Tree` and `TreeItem`, with selected, expanded and
    level set. On Linux, where AccessKit's AT-SPI adapter maps neither, the
    row's description also says "expanded" or "collapsed" and "level N".
    LATER: drop that once AccessKit maps them.
  - **Icons that carry meaning** (dirty dot, conflict, warning, spinner;
    UI-3) have a name.
  - **Dialogs:** each is named from its title, with its text as its
    description, and is modal (through synthetic children, finding 3). The
    shared frame in `dialog.rs` sets all three.
  - **Known gaps** are allowed only per element, with a reason in the
    checker. A gap by role isn't allowed.
- **The live status** (M5-10 "Announcements"):
  - **One node:** a polite, live `Status` node, which starts empty
    (finding 4).
  - **What it announces:** a job that ends failed or with a conflict, and
    the update notice. Progress isn't announced.
  - **The other nodes:** today's `Role::Status` nodes (palette, banner,
    preflight, job list, tab bar, missing repos, Resolve all) stop being
    live, or say why they must stay.
  - **Test:** the dump doesn't hold live nodes, so this is checked through
    AT-SPI. The spike's probe is ported for this.
- **Fix what the checks find.** The pull request lists each finding and its
  fix.

## Tests (QA-3)
- **The harness above, over every view.** It fails on the first view that
  breaks a rule, naming the node and the rule.
- **Checker unit tests,** one per rule. Each has a dump that breaks the
  rule, and the checker must name it.
- **`ui_21_a_failed_job_is_announced_once_and_progress_is_not`**, through
  AT-SPI.
- **`ui_22_window_buttons_are_named_and_not_tab_stops`**.

## Out of scope
- **Focus, motion and text size:** M6-25b.
- **Text fields exposing their text:** M6-25c.
- **Windows and macOS trees:** they activate only when UIA or the AX API
  asks, so they're checked by hand with M6-25c's scripts.
- **An axe-like WCAG rule set:** there is none for GPUI (README decisions
  table).

## Acceptance
- The harness runs in CI on Linux and passes, and it fails when a button's
  name is removed.
- The pull request lists each finding and its fix, and each allowed gap with
  its reason.
- `just check` passes.
