// What the tab bar's "+" offers (UI-28): every workspace file, open ones
// marked, to open or switch to (UI-27); New workspace; Import workspace,
// disabled until import exists (M3-14); and Delete for a workspace that is
// not open. The same entries show as a menu below "+" and as buttons when no
// tab is open. The core lists, sorts and opens; this shows what it says.
import { createEffect, createResource, For, type JSX, on, onCleanup, Show } from "solid-js";
import type { ConfigWarning, WorkspaceListItem } from "../ipc";
import type { WorkspaceStore } from "../stores/workspaces";
import type { WorkspaceDialogs } from "./WorkspaceDialogs";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** Why a workspace file opens read-only (DATA-1). */
function warningText(warning: ConfigWarning): string {
  switch (warning.kind) {
    case "newer_schema":
      return `From a newer gitcroc (schema ${warning.found}); it opens read-only.`;
    case "unreadable":
      return `Cannot be read: ${warning.reason}. It opens read-only.`;
    case "no_config_dir":
      return "There is no config directory; changes are not saved.";
  }
}

function Entries(props: {
  role: "menuitem" | undefined;
  items: WorkspaceListItem[];
  onOpen: (item: WorkspaceListItem) => void;
  onDelete: (item: WorkspaceListItem) => void;
  onNew: () => void;
}) {
  return (
    <>
      <For each={props.items}>
        {(item) => (
          <div class="workspace-entry">
            <button
              type="button"
              role={props.role}
              class="workspace-open-button"
              title={item.warning === null ? undefined : warningText(item.warning)}
              onClick={() => props.onOpen(item)}
            >
              <span class="workspace-name">{item.name}</span>
              <Show when={item.open}>
                <span class="workspace-mark"> (open)</span>
              </Show>
              <Show when={item.warning !== null}>
                <span class="workspace-mark"> (read-only)</span>
              </Show>
            </button>
            <Show when={!item.open}>
              <button
                type="button"
                role={props.role}
                class="workspace-delete"
                aria-label={`Delete ${item.name}…`}
                title={`Delete the workspace ${item.name}`}
                onClick={() => props.onDelete(item)}
              >
                Delete…
              </button>
            </Show>
          </div>
        )}
      </For>
      <hr class="menu-separator" />
      <button type="button" role={props.role} onClick={() => props.onNew()}>
        New workspace…
      </button>
      <button type="button" role={props.role} disabled title="Arrives with import and export of workspaces">
        Import workspace…
      </button>
    </>
  );
}

interface MenuProps {
  workspaces: WorkspaceStore;
  dialogs: WorkspaceDialogs;
  /** Says why a workspace did not open. */
  onError: (message: string) => void;
  /** Called once an entry was chosen. */
  onClose?: () => void;
}

/** The workspace files, read anew each time they are shown so files created elsewhere show, and what choosing does. */
function createEntries(props: MenuProps) {
  const [items, { refetch }] = createResource(() => props.workspaces.list());
  const done = () => props.onClose?.();

  const open = (item: WorkspaceListItem) => {
    done();
    props.workspaces.open(item.id).catch((error) => {
      props.onError(`Cannot open ${item.name}: ${message(error)}`);
      void refetch();
    });
  };
  const remove = (item: WorkspaceListItem) => {
    done();
    props.dialogs.open({ kind: "delete", id: item.id, name: item.name });
  };
  const create = () => {
    done();
    props.dialogs.open({ kind: "new" });
  };

  const entries = (role: "menuitem" | undefined): JSX.Element => (
    <>
      <Show when={items.error as unknown}>
        <p role="alert">Cannot list the workspaces: {message(items.error)}</p>
      </Show>
      <Entries role={role} items={items.latest ?? []} onOpen={open} onDelete={remove} onNew={create} />
    </>
  );
  return { items, done, entries };
}

/** The entries in place, as buttons, when no tab is open. */
export function WorkspaceStart(props: MenuProps) {
  const { entries } = createEntries(props);
  return (
    <div class="workspace-start" role="group" aria-label="Workspaces">
      {entries(undefined)}
    </div>
  );
}

/** The entries as a menu below "+", its right edge `right` pixels from the window's, since "+" is at the far right. */
export default function WorkspaceMenu(props: MenuProps & { at: { right: number; top: number } }) {
  const { items, done, entries } = createEntries(props);
  const closeOnEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      done();
    }
  };
  document.addEventListener("keydown", closeOnEscape);
  onCleanup(() => document.removeEventListener("keydown", closeOnEscape));
  let menu: HTMLDivElement | undefined;
  // The first entry has the focus once the list is in.
  createEffect(
    on(
      () => items.loading,
      (loading) => {
        if (!loading) {
          menu?.querySelector<HTMLElement>("[role=menuitem]:not(:disabled)")?.focus();
        }
      },
    ),
  );
  return (
    <div class="menu-backdrop" onClick={done} onContextMenu={(event) => event.preventDefault()}>
      <div
        class="menu workspace-menu"
        role="menu"
        aria-label="Open workspace"
        style={{ right: `${props.at.right}px`, top: `${props.at.top}px` }}
        ref={(element) => (menu = element)}
        onClick={(event) => event.stopPropagation()}
      >
        {entries("menuitem")}
      </div>
    </div>
  );
}
