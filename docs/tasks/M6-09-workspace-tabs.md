# M6-09 Workspace tabs and workspace dialogs

## Goal
`gitcroc-next` has rc.1's tab bar in its title bar:
- one tab per open workspace;
- "+", which lists the workspace files and offers New workspace and Import workspace;
- reordering by dragging and from the keyboard;
- closing;
- the New, Rename and Delete dialogs.

With every tab closed, the window offers the workspaces in place. The menus and dialogs built here are the ones later tasks reuse.

## Depends on
- M6-06: the window, its title bar, its event task, the strings module and the keymap.
- M6-07: the tokens the tab bar, menus and dialogs draw with.

## Spec
Implements: UI-27, UI-28
Respects: ARCH-6, ARCH-7, ARCH-9, UI-22, UI-23, UI-33, UI-35, D15, DATA-1, SEC-3, QA-3, QA-4

## In scope
- **The workspaces stay in `gitcroc-app`** (ARCH-6). The core checks names, opens, orders and saves; the UI shows what it reports:
  - `workspace_get` and the `WorkspacesChanged` and `WorkspaceChanged` events, for the tabs;
  - `workspace_list`, for the "+" menu;
  - `workspace_create`, `workspace_open`, `workspace_close`, `workspace_rename`, `workspace_activate`, `workspace_reorder` and `workspace_delete`;
  - `workspace_export` and `workspace_import`, which open the app's file dialogs (SEC-2).

  No service changes. A call that returns the tabs shows them at once, as its event would.
- **The window's root** becomes gpui-component's `Root`, around the window's view:
  - Tab and Shift+Tab move the focus;
  - a focus trap keeps it inside a dialog.

  The headless test window gets the same root.
- **Shared menu** (`crates/gitcroc-ui/src/menu.rs`): a view of its own, because gpui-component's popup menu opens only from its own triggers, and Shift+F10 must open it too.
  - **Roles and names:** the menu has a name. Its entries are menu items named by their text, or by a longer label where rc.1 had one ("Delete archive…").
  - **Position:** it opens at a point, below "+" or at the pointer, and stays inside the window.
  - **Keys:**
    - the first enabled entry has the focus;
    - Up, Down, Home and End move, wrapping around;
    - Enter and Space choose;
    - Escape closes.
  - **Closing:** a click outside closes it. The focus goes back to what opened it, unless the entry opened a dialog.
- **Shared dialogs** (`crates/gitcroc-ui/src/dialog.rs`), ports of `ui/src/dialogs/parts.tsx`:
  - **Frame:** a modal over the window, with role dialog or alert dialog and its title as its name. Escape closes it, and the focus is trapped inside.
  - **Name dialog:**
    - a Name field (gpui-component's input), filled with the current name for a rename;
    - Cancel, and a submit button that Enter also presses;
    - while it sends, the submit button is disabled;
    - the core's refusal shows under the field as an alert, and typing clears it;
    - on success the dialog closes.
  - **Question:**
    - a title, its text, Cancel and a danger button;
    - Cancel has the focus;
    - the refusal shows as an alert;
    - on success it closes.
- **The tab bar** (`crates/gitcroc-ui/src/tab_bar.rs`), in the title bar row in place of the title (UI-35):
  - **Tabs:**
    - one per open workspace, in the core's order, named by the workspace's name;
    - the active one is selected, and it alone is in the Tab order;
    - a click activates a tab.
  - **Closing:** each tab has a close button for the mouse, outside the Tab order. Delete stands for it from the keyboard. Closing never deletes the file (UI-28).
  - **Reordering:** a tab dropped on another takes its place, sent as `workspace_reorder`.
  - **Keys on a tab:**
    - Left and Right activate the tab beside;
    - Home and End, the first and the last;
    - Shift+F10 and the context menu key open its menu.
  - **Keys anywhere in the window**, not while a dialog or menu is open (UI-22):
    - Ctrl/Cmd+PageDown and PageUp activate the next and the previous tab;
    - with Shift, they move the active tab.

    These four go in `keys::KEYMAP`, and `docs/user/keyboard.md` already lists them.
  - **The tab's menu:** Rename…, Export workspace… and Close.
  - **"+":** opens the workspace menu below it.
  - **Refusals:** a tab change the core refuses shows below the title bar as a dismissible alert.
  - **Moving the window:** the empty space after "+" still moves and maximizes the window. A press on a tab or a button does not.
- **The workspace menu**, a port of `ui/src/workspaces/WorkspaceMenu.tsx`:
  - **Listed anew each time it opens:** every workspace file, sorted by the core, with these marks:
    - "(open)" on an open one;
    - "(read-only)" on one with a warning (DATA-1), with the warning as its description.
  - **Delete…** follows each workspace that is not open.
  - **Then:** New workspace… and Import workspace….
  - **Choosing a workspace** opens it as the last tab, or switches to its tab (UI-27). When it cannot be opened, the reason shows as an alert and the list is read again.
  - **Errors:** a list that cannot be read shows why, as an alert.
- **The start screen:** with no tab open, the window says that no workspace is open, and shows the same entries as buttons in a group named "Workspaces" (UI-27). The panes, toolbar and status bar are not shown then.
- **The workspace dialogs**, a port of `ui/src/workspaces/WorkspaceDialogs.tsx`:
  - **New workspace:** creates it, and it opens as the active last tab.
  - **Rename *name*:** renames it.
  - **Delete the workspace *name*?:** says that the repositories on disk are not touched. Delete removes the file, with `confirmed` set.

  There is at most one dialog at a time.
- **Missing workspace files** (UI-33): when tabs were not restored because their files were gone, a dismissible alert names the files.
- **Regions (UI-22):** F6's first region is the tab bar. Its focus lands on the active tab, or on "+" when no tab is open.
- **Names are text** (SEC-3). Every colour comes from the tokens (UI-23).
- **Strings (D15):** `tabs`, `workspaces`, `common`, `dialogs.name`, `app.noWorkspace` and `app.missingWorkspaces` move from `ui/src/strings/en.ts` to `strings.rs`, with the same text.
- **Test support:** a fixture can start with several workspace files and saved tabs. A test reads the files and tabs through the app.

## Tests (QA-3)
- `ui_27_tabs_follow_the_open_workspaces_and_the_active_one`:
  - the tabs show the open workspaces in order, the active one selected, in the title bar;
  - the tabs follow a change the app makes elsewhere, and a rename.
- `ui_27_with_every_tab_closed_the_window_offers_the_workspaces`:
  - the window says no workspace is open and shows no panes;
  - the start screen lists the workspaces, New and Import;
  - choosing one opens it and shows the panes.
- `ui_28_plus_menu_lists_workspaces_and_offers_new_and_import`:
  - the entries are in order, with the open one marked and Delete… only for closed ones;
  - the first entry has the focus;
  - choosing a closed one opens it as the active last tab;
  - choosing an open one switches to it;
  - Escape closes the menu and gives the focus back to "+".
- `ui_28_new_and_rename_show_the_core_refusal_inline`:
  - New: a duplicate and a blank name show the core's refusal; a good name opens the active last tab;
  - Rename: the field starts with the name; a duplicate is refused; a good name renames the tab.
- `ui_28_delete_asks_first_and_leaves_the_repos`:
  - the question names the workspace and the repositories;
  - Cancel has the focus;
  - nothing is deleted before Delete, and the file is gone after it.
- `ui_28_tabs_reorder_by_drag_and_close`:
  - dragging a tab onto the first puts it first, and the core keeps the order;
  - the close button and the tab menu's Close close tabs;
  - closing the active tab activates the one in its place;
  - the files stay.
- `ui_22_tabs_switch_and_move_from_the_keyboard`:
  - Ctrl+Shift+PageDown and PageUp move the active tab, and stop at the ends;
  - Ctrl+PageDown and PageUp, the arrow keys, Home and End switch tabs;
  - Delete closes the focused tab;
  - Shift+F10 opens its menu.
- `ui_22_dialogs_trap_the_focus_and_close_with_escape`:
  - Tab stays inside the name dialog;
  - Escape closes it;
  - no tab shortcut works while it is open.
- `ui_33_missing_workspace_files_are_named_until_dismissed`.
- `ui_35_the_tab_bar_is_the_title_bar`:
  - a press on a tab does not move the window, and a press on the empty space does;
  - the setup screen still shows the app's name and no tabs.
- `sec_3_workspace_names_render_as_text`: a name with markup shows as written in the tab, the menu and the dialogs.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module` and the keymap docs test keep passing over the new files.

## Out of scope
- **The tab's job badge and failure marker (UI-32):** M6-15, with the job list.
- **The tab menu's AI switch (AI-6, AI-16):** M6-23.
- **Closing a tab that holds unsaved conflict edits (OPS-9):** M6-19 adds the question.
- **What an export or import reports, and the missing repos dialog (DATA-2):** M6-20. Here the entries only start them, and a refused start shows as an alert.
- **Per-tab pane state (UI-29) and background tabs (UI-30, UI-31):** with the panes, M6-10 to M6-13. Pane sizes are already kept per workspace (M6-06).
- **The palette's next and previous tab:** M6-24.

## Acceptance
- **On the reference machine,** `just dev-next` with two or more workspaces:
  - opens the tabs as rc.1 left them;
  - creates, renames, reorders, closes, reopens and deletes workspaces from the tab bar;
  - still moves the window by the empty space of the title bar.
- `just coverage M6` no longer lists UI-27 or UI-28.
- `just check` passes.
