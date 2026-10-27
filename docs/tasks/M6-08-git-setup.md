# M6-08 Git setup screen and git path

## Goal
`gitcroc-next` behaves like 1.0.0-rc.1 while no usable git is found (PLAT-3, D13):
- a full-window setup screen says why;
- **Check again** detects git again without a restart;
- **Choose git…** picks the executable in the file dialog;
- **Use automatic detection** gives a chosen git up.

Until the first detection ends, the window says it is looking for git. Once git is usable, the main window shows, with no restart.

## Depends on
- M6-06: the window, its event task, the title bar, the strings module and the file dialogs.
- M6-07: the tokens and the code font the screen draws with.

## Spec
Implements: PLAT-3
Respects: ARCH-6, ARCH-7, D13, D15, SEC-2, SEC-3, UI-35, QA-3, QA-4

## In scope
- **Detection stays in `gitcroc-app`** (ARCH-6). The UI only shows what the app reports:
  - `git_status` and the `GitStatus` events, for what was found;
  - `git_recheck`, to detect again;
  - `git_pick_path`, to choose git in a file dialog;
  - `settings_set_git_path(None)`, for automatic detection.
  
  No service changes.
- **The window's git state** in `window.rs`, mirroring rc.1's `stores/git.ts`:
  - **Fields:**
    - the latest finished result, `None` until the first one;
    - whether a detection is running;
    - why the last choice failed.
  - **Updates:**
    - A `Checking` event sets "running" and keeps the result.
    - A finished result replaces the result and clears "running".
  - **First read:** the window reads `git_status` once at start, in a task (ARCH-7). An event that arrived before that read is newer, so the read is then dropped.
  - **After lag:** when events were dropped, the status is read again with the workspaces.
- **What the window shows:**
  - **No result yet:** the title bar with the app's name, and "Looking for git…" (the message screen).
  - **A result that is not usable:** the title bar with the app's name (UI-35) and the setup screen. The toolbar, the panes and the status bar are not built into the frame.
  - **A usable result:** the main window as M6-06 and M6-07 made it.
  - **While a detection is running:** the screen stays as it is. Only the setup screen's buttons change (below).
  - **F6 (UI-22):** it moves among the regions on screen: the title bar and the setup screen while that is shown.
  - **Focus:** when the setup screen appears, its first button gets it.
- **The setup screen** in `crates/gitcroc-ui/src/git_setup.rs`: a view over one problem result, the OS from `app_info`, "running" and the error. It ports `ui/src/GitSetup.tsx`:
  - **Heading:** "gitcroc needs git".
  - **The problem, as an alert:**
    - **Not found:**
      - "git was not found.", then "Looked for it at:" and the searched paths as a list;
      - with no paths, "There was nowhere to look: PATH is empty." instead.
    - **Too old:** the found version, the path and the required version.
    - **Unparseable:** the path, and what git printed, in the code font.
  - **Missing chosen git:** with `chosen_missing`, "The chosen git was not found at …".
  - **Install hint:** for the OS (Windows, macOS, Linux), or the default hint.
  - **Errors:** why "Use automatic detection" failed, such as read-only settings (DATA-1), as an alert.
  - **Buttons:**
    - **Check again:** while a detection runs, it reads "Checking…".
    - **Choose git…**
    - **Use automatic detection:** only when a git was chosen.
    
    All three are disabled while a detection runs. They are gpui-component buttons, so Tab, Enter and Space work.
  - **Long content:** the screen scrolls when it is taller than the window, as a long Windows search list can be.
  - **Colours:** tokens only (UI-23).
  - **SEC-3:** paths, versions and git's output are drawn as text. GPUI has no markup, and the test keeps it so.
  - **Accessibility:** each line has an id for the tests and a role (heading, alert, list item, paragraph). Its label is its text, so tests and screen readers read the same thing.
- **Strings (D15):** `gitSetup` and `app.lookingForGit` move from `ui/src/strings/en.ts` to `strings::git_setup`, with the same text.
- **Test support:**
  - **Fixture options:** the fixture can start the app:
    - with a `git_path` in its settings file;
    - without waiting for detection;
    - with dialogs that answer a given path.
  - **Fake gits:** `fake_git` writes a script that prints a version, as the core's detection tests do. On Windows, a `.cmd`.
  - **Blocking fake git:** one waits until the test creates a file, so a test sees "Looking for git…" without depending on timing.

## Tests (QA-3)
- `plat_3_looking_for_git_until_the_first_result`:
  - while a chosen fake git has not answered, the window shows "Looking for git…" and no toolbar;
  - once it answers usable, the main window shows.
- `plat_3_too_old_shows_both_versions_and_check_again_opens_the_app`:
  - a chosen git that is too old shows the alert with both versions, and no toolbar;
  - after the script prints a new enough version, **Check again** shows the main window.
- `plat_3_not_found_lists_the_searched_paths_and_an_install_hint`: a not-found result with a Windows path and OS `windows` lists the path and the winget hint.
- `plat_3_not_found_with_nowhere_to_look`.
- `plat_3_choose_git_gets_past_the_setup_screen`:
  - without a chosen git, **Use automatic detection** is absent;
  - **Choose git…**, answered with a usable fake git, shows the main window and saves the path.
- `plat_3_a_missing_chosen_git_is_named_and_can_be_given_up`: with a chosen git that does not exist and a not-found result:
  - the screen names the path;
  - **Use automatic detection** clears the setting.
- `plat_3_buttons_are_disabled_while_checking`: during a detection, every button is disabled and the first reads "Checking…".
- `plat_3_read_only_settings_refuse_automatic_detection`: the error shows as an alert.
- `plat_3_setup_screen_regions_and_focus`: F6 moves between the title bar and the screen, and the first button has the focus.
- `sec_3_git_path_and_output_render_as_text`: a path and output with markup show as written.
- `ui_23_no_hardcoded_colors` and `d15_ui_text_comes_from_the_strings_module` keep passing over the new file.

## Out of scope
- The Settings dialog's Git section, with the same choose and automatic actions (M6-26).
- Detection itself, the Git for Windows locations and the version check, which `gitcroc-core` and `gitcroc-git` already test.

## Acceptance
- **On the reference machine,** `just dev-next` behaves as follows:
  - with `"git_path"` set to a missing file and git on PATH, it opens the main window;
  - with a chosen git that prints `git version 2.30.0`, it opens the setup screen;
  - **Use automatic detection** then opens the main window without a restart.
- `just coverage M6` no longer lists PLAT-3 or SEC-3.
- `just check` passes.
