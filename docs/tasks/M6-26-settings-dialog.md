# M6-26 The Settings dialog, all four tabs

## Goal
The Settings dialog that M6-23a opened with only its AI tab gets rc.1's
other three: Appearance, Git and Updates, in that order before AI. The
user picks the theme, the code themes with a preview of each, the fonts and
their sizes and the system title bar; chooses git by hand or goes back to
automatic detection; sets how many repos a group operation runs at once and
whether the preflight dialog always shows; and turns the update check on or
off and checks now. Each change applies at once and is saved by the core.
Read-only settings show why, in the window and in the dialog, and every
control is disabled.

## Depends on
- M6-23a: the dialog's frame, its tab strip and the AI tab.
- M6-07: `AppTheme::apply_settings`, the code themes and the fonts.
- M6-08: the git setup screen's choose and automatic actions.
- M6-15b: `Updates`, the window's view of the updater.
- M6-24a: the palette's Settings and Check for updates.

## Spec
Implements: UI-23, UI-24, UI-26 and UI-35 (their settings), PLAT-3 (the
Settings part), D12 (the Updates section), DATA-1 (the read-only warning),
section 5 steps 2 and 3 (the group settings)
Respects: UI-22, UI-25, OPS-9, SEC-3, SEC-7, ARCH-6, ARCH-7, D15, QA-2,
QA-3, QA-4

The services are done (M6-05): `settings_get`, `settings_set_theme`,
`settings_set_appearance`, `settings_set_groups`,
`settings_set_update_check`, `settings_set_git_path`, `git_pick_path`,
`git_status`, `update_status` and `update_check_now`, with `GitStatus` and
`UpdateStatusChanged` on the event channel. The core limits font sizes to
8 to 32 and refuses a parallelism outside 1 to 32. The dialog shows the
`SettingsView` each call returns and keeps no copy of its own (ARCH-6).

## In scope
- **The tab strip:** Appearance, Git, Updates, AI, as rc.1. The dialog
  opens at Appearance, or at the tab a caller names: AI for AI-3's "open
  the provider settings", Updates for the palette's Check for updates.
  M6-23a's keys (arrows, Home, End, focus to the tab's first control) cover
  the new tabs.
- **Appearance**, a port of `ui/src/SettingsDialog.tsx`'s first panel:
  - **Theme** (UI-23): Follow the system, Light, Dark, through
    `settings_set_theme`.
  - **Code themes** (UI-24): one choice for light mode and one for dark,
    from the eight the app ships, each with its own preview. The preview is
    a port of `ui/src/theme/CodeThemePreview.tsx`: a hunk header, a context
    line, a removed and an added line, highlighted in the chosen theme
    whatever the app theme is, and hidden from screen readers.
  - **Fonts** (UI-26): the UI font and the code font as text fields, empty
    for the system font, as rc.1; and each size from 8 to 32 px. A size
    that is not a number puts the saved one back; the core moves the rest
    to the nearest limit. A font that is not installed falls back as
    M6-07 does.
  - **Use the system title bar** (UI-35), which applies at once:
    - **On Linux** the window switches its decorations in place, through
      `request_decorations`, and the title bar row follows.
    - **On Windows and macOS** GPUI fixes the decorations when the window
      opens, so the app closes the window and opens a new one at the same
      bounds with the same tabs and the same active tab, which the core
      holds. The panes' scroll, the focus and any open dialog are not kept.
    - **With unsaved conflict edits** in any tab (OPS-9), reopening asks
      first, listing them, as Install and restart does (M6-15b). Cancel
      leaves the setting as it was.
    - Both go through M6-06's `WindowControls`, so a test asks for the
      switch without a real window.
  - Every change goes through `settings_set_appearance` or
    `settings_set_theme`, and the returned view reaches the theme through
    one `AppTheme::apply_settings`.
- **Git**, the panel's two sections:
  - **Git** (PLAT-3): the git in use, its version and path; when the chosen
    git is missing, the message that names it and the git used instead.
    **Choose git…** and, with a chosen git, **Use automatic detection**,
    the same actions as the setup screen (M6-08), disabled while git is
    being checked. The window's `GitStatus` feeds the panel; it keeps no
    copy.
  - **Group operations** (section 5): **Repos at once**, 1 to 32, and
    **Always show the preflight dialog**, through `settings_set_groups`. A
    refused value shows the core's message and the field goes back to the
    saved value.
- **Updates** (D12, SEC-7), a port of `ui/src/update/UpdateSection.tsx`:
  - **Check for updates automatically**, through
    `settings_set_update_check`.
  - **The last result with its time:** not checked yet, checking, up to
    date, available, failed with its reason, downloading, installed and
    restarting. A failed check shows here and nowhere else. A build that
    does not check says so.
  - **Check now**, which also runs while automatic checks are off. It is
    disabled while a check, a download or a restart is under way, and in a
    build that does not check.
- **Read-only settings** (DATA-1): when `SettingsView` has a warning, a
  banner in the window says why, as rc.1's `ui/src/SettingsBanner.tsx`
  does: a newer schema, an unreadable file or no config directory, with the
  path. In the dialog every control of the four tabs is disabled and shows
  the saved value; the AI tab's controls too. Paths and reasons are text
  only (SEC-3).
- **The palette** (M6-24a): **Settings** opens the dialog at Appearance.
  **Check for updates** opens the update dialog when a version is on offer,
  as now; otherwise it opens Settings at the Updates tab and checks now.
  Its "No newer version is on offer" reason goes.
- **The accessibility tree test** (M6-25a): the Settings step visits every
  tab and checks their controls' roles, names and states.
- **Strings (D15):** the rest of `settings` (`themes`, the fonts, the
  window, git, group operations, updates and the three warnings) moves from
  `ui/src/strings/en.ts` to `strings.rs`.

## Tests (QA-3)
GPUI tests against the real core and its settings file in a temporary
directory (QA-2), with `support::Options` for git detection and the update
status. No test waits on timing.
- `ui_23_the_theme_choice_applies_and_is_saved`: Dark applies the dark
  tokens and is in the saved file; Follow the system follows a system
  change live.
- `ui_24_each_code_theme_choice_shows_its_own_preview`: the light-mode
  preview keeps its theme's colors while the app is dark, and changing a
  choice changes only its own preview.
- `ui_24_the_chosen_code_theme_applies_to_the_diff`.
- `ui_26_fonts_and_sizes_apply_and_are_saved`: a size of 40 is saved as 32,
  and text that is not a number puts the saved size back.
- `ui_35_the_setting_switches_the_title_bar_at_once`: on Linux the window
  is asked for system decorations in place and the window buttons go; on
  Windows and macOS it is reopened with the same tabs, the same active tab
  and the same bounds.
- `ui_35_reopening_asks_first_with_unsaved_conflict_edits`: the question
  lists them, Cancel keeps the setting off and the window, and confirming
  reopens it.
- `plat_3_settings_shows_the_git_in_use`.
- `plat_3_settings_names_a_missing_chosen_git_and_can_go_back`.
- `plat_3_a_cancelled_git_dialog_changes_nothing`.
- `the_group_settings_round_trip_and_outside_1_to_32_is_refused` (section
  5 has no ID for its settings, as in rc.1's test):
  12 is saved; 40 shows the core's message and the field shows 12;
  Always show the preflight dialog makes the next group fetch ask.
- `d12_settings_turns_the_automatic_check_off_and_checks_now`.
- `d12_a_failed_check_shows_in_settings_with_its_time_and_nowhere_else`.
- `d12_a_build_that_does_not_check_says_so_and_cannot_check_now`.
- `d12_check_for_updates_in_the_palette_opens_the_updates_tab`.
- `sec_7_read_only_settings_keep_the_automatic_check_as_it_is`.
- `data_1_read_only_settings_show_a_banner_and_disable_every_tab`: with a
  settings file of a newer schema, the banner names the path and the
  schema, and no control of any tab is enabled.
- `sec_3_settings_warning_and_git_path_render_as_text`.
- `ui_22_every_settings_tab_is_keyboard_reachable`: the tab keys reach all
  four, and Tab reaches each control in its order.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`,
  the keymap docs test and `just a11y` keep passing over the new files.

## Out of scope
- **The update source and installing** (PLAT-4): M6-27.
- **A font picker listing the installed fonts:** rc.1 has text fields;
  LATER.
- **Keeping the panes' scroll and focus across a reopened window.**

## Acceptance
- **On the reference machine (Linux),** `just dev-next`: switch the theme,
  each code theme and both fonts and see them apply; turn the system title
  bar on and off and see the window switch without restarting; choose a git
  by hand and go back to automatic; set 2 repos at once and see a group
  fetch run two at a time; check for updates now. Restart, and everything
  is kept.
- **The title bar on Windows and macOS** is checked by hand at M6-31's
  sign-off, with M6-28's packages.
- `just check` passes.
