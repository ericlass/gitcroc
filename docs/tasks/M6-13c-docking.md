# M6-13c Docking the changes pane

## Goal
The changes pane docks right, by default, or at the bottom under the
history, as rc.1's does. The choice and the pane sizes are kept per
workspace, and every pane stays in the window when it gets smaller.

This is the last of three parts of M6-13.

## Depends on
- M6-13a: the pane header the dock button sits in.

## Spec
Implements: UI-19
Respects: UI-29, ARCH-6, ARCH-7, ARCH-9, D15, QA-3, QA-4

## In scope
- **The dock button** (UI-19) at the end of the pane header: "Dock at
  bottom" while docked right, "Dock right" while at the bottom.
- **The layout**, as `ui/src/layout` and `ui/src/stores/layout.ts` arrange it:
  - docked right: tree, history and changes side by side, as today;
  - docked at the bottom: the tree on the left, and the history above the
    changes pane on the right, with a splitter between them.
- **Kept per workspace** (UI-19, section 10): the dock, and the changes
  pane's height while at the bottom, are saved with
  `workspace_set_layout` in the workspace's `PaneLayout`, beside the tree
  and history widths saved today. A tab shows its own layout (UI-29).
- **Staying in the window**, as rc.1's `ui/e2e/layout.spec.ts` checks:
  - each pane keeps a minimum width of 200 px, and the history and the
    changes pane at the bottom a minimum height of 120 px, as rc.1's
    `MIN_PANE_WIDTH` and `MIN_PANE_HEIGHT`;
  - when the window shrinks, the panes shrink to fit and the saved sizes
    are kept, so a larger window shows them again;
  - a drag starts from the size shown.
- **The splitters' keys:** a focused splitter moves with the arrow keys,
  as in rc.1.
- **Strings (D15):** the dock button's labels move to `strings.rs`.

## Tests (QA-3)
- `ui_19_the_dock_button_moves_the_pane_and_is_kept`: right by default;
  the button docks at the bottom and back; the choice is in the workspace
  file.
- `ui_19_dock_is_per_workspace`: a second tab keeps its own dock.
- `ui_19_the_changes_pane_stays_in_the_window`: docked right and at the
  bottom, a smaller window keeps every pane at its minimum, and a larger
  one shows the saved sizes again.

## Out of scope
- **Background tabs releasing their state** (UI-31): M6-14.

## Acceptance
- **On the reference machine,** `just dev-next`: dock at the bottom,
  resize, switch tabs, restart; each tab comes back with its layout.
- `just coverage M6` no longer lists UI-19.
- `just check` passes.
