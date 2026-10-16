// The workspace tabs (UI-27, UI-28): one tab per open workspace in the
// core's order. A click activates a tab; tabs reorder by dragging and with
// Ctrl/Cmd+Shift+PageUp/PageDown (UI-22), and the arrow keys move between
// them. Each tab has a close button and a context menu with Rename,
// Export workspace (DATA-2), AI for this workspace (AI-6), shown only
// while AI features are on (AI-16), and Close. "+" lists the workspace files and offers New and Import. A tab
// shows how many of its jobs run or wait, and a marker for a failed or
// conflicted job the user has not seen (UI-32). The core keeps the tabs;
// this sends what the user did.
import { createSignal, For, onCleanup, Show } from "solid-js";
import type { WorkspaceId } from "../ipc";
import type { AiStore } from "../stores/ai";
import type { JobStore } from "../stores/jobs";
import type { ShareStore } from "../stores/share";
import type { WorkspaceStore } from "../stores/workspaces";
import type { WorkspaceDialogs } from "../workspaces/WorkspaceDialogs";
import WorkspaceMenu from "../workspaces/WorkspaceMenu";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export default function TabBar(props: {
  workspaces: WorkspaceStore;
  jobs: JobStore;
  dialogs: WorkspaceDialogs;
  share: ShareStore;
  ai: AiStore;
  /** Closes a tab; the caller asks first when it holds unsaved conflict edits (OPS-9). */
  onClose: (id: WorkspaceId) => void;
}) {
  const [plusMenu, setPlusMenu] = createSignal<{ right: number; top: number } | null>(null);
  const [tabMenu, setTabMenu] = createSignal<{ id: WorkspaceId; name: string; x: number; y: number } | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  /** The tab being dragged. */
  const [dragged, setDragged] = createSignal<WorkspaceId | null>(null);
  let list: HTMLDivElement | undefined;

  const views = () => props.workspaces.state.views ?? [];
  const active = () => props.workspaces.state.active;

  /** Sends a tab change; a refusal shows below the tabs. */
  const send = (request: Promise<unknown>) => {
    setError(null);
    request.catch((failure) => setError(message(failure)));
  };

  const focusTab = (id: WorkspaceId) =>
    queueMicrotask(() => list?.querySelector<HTMLElement>(`[role=tab][data-id="${CSS.escape(id)}"]`)?.focus());

  /** Moves `id` by `step` places, keeping the focus on it. */
  const move = (id: WorkspaceId, step: number) => {
    send(props.workspaces.move(id, step).then(() => focusTab(id)));
  };

  // Ctrl/Cmd+Shift+PageUp/PageDown moves the active tab wherever the focus is.
  const onWindowKey = (event: KeyboardEvent) => {
    const id = active();
    if (id === null || !(event.ctrlKey || event.metaKey) || !event.shiftKey) {
      return;
    }
    if (event.key === "PageUp" || event.key === "PageDown") {
      event.preventDefault();
      move(id, event.key === "PageUp" ? -1 : 1);
    }
  };
  window.addEventListener("keydown", onWindowKey);
  onCleanup(() => window.removeEventListener("keydown", onWindowKey));

  /** The arrow keys, Home and End activate another tab; the context menu key opens its menu. */
  const onTabKey = (event: KeyboardEvent, id: WorkspaceId, name: string) => {
    const order = views().map((view) => view.id);
    const index = order.indexOf(id);
    let next: WorkspaceId | undefined;
    switch (event.key) {
      case "ArrowLeft":
        next = order[index - 1];
        break;
      case "ArrowRight":
        next = order[index + 1];
        break;
      case "Home":
        next = order[0];
        break;
      case "End":
        next = order[order.length - 1];
        break;
      case "ContextMenu":
      case "F10":
        if (event.key === "F10" && !event.shiftKey) {
          return;
        }
        event.preventDefault();
        openTabMenu(event.currentTarget as HTMLElement, id, name);
        return;
      default:
        return;
    }
    event.preventDefault();
    if (next !== undefined) {
      const target = next;
      send(props.workspaces.activate(target).then(() => focusTab(target)));
    }
  };

  const openTabMenu = (element: HTMLElement, id: WorkspaceId, name: string, x?: number, y?: number) => {
    const box = element.getBoundingClientRect();
    setTabMenu({ id, name, x: x ?? box.left, y: y ?? box.bottom });
  };

  const closeOnEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      setTabMenu(null);
    }
  };
  document.addEventListener("keydown", closeOnEscape);
  onCleanup(() => document.removeEventListener("keydown", closeOnEscape));

  /** Drops the dragged tab where `target` is. */
  const drop = (target: WorkspaceId) => {
    const id = dragged();
    setDragged(null);
    if (id === null || id === target) {
      return;
    }
    const order = views()
      .map((view) => view.id)
      .filter((other) => other !== id);
    const to = views().findIndex((view) => view.id === target);
    order.splice(to, 0, id);
    send(props.workspaces.reorder(order));
  };

  return (
    <>
      <div class="tab-bar">
        <div role="tablist" aria-label="Workspaces" class="tabs" ref={(element) => (list = element)}>
          <For each={views()}>
            {(view) => (
              <div
                class="tab-item"
                classList={{ "tab-dragged": dragged() === view.id }}
                draggable="true"
                onDragStart={(event) => {
                  setDragged(view.id);
                  if (event.dataTransfer) {
                    event.dataTransfer.effectAllowed = "move";
                    event.dataTransfer.setData("text/plain", view.name);
                  }
                }}
                onDragOver={(event) => {
                  if (dragged() !== null) {
                    event.preventDefault();
                  }
                }}
                onDrop={(event) => {
                  event.preventDefault();
                  drop(view.id);
                }}
                onDragEnd={() => setDragged(null)}
              >
                <button
                  type="button"
                  role="tab"
                  class="tab"
                  data-id={view.id}
                  aria-selected={view.id === active()}
                  tabindex={view.id === active() ? 0 : -1}
                  title={view.name}
                  onClick={() => send(props.workspaces.activate(view.id))}
                  onKeyDown={(event) => onTabKey(event, view.id, view.name)}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    openTabMenu(event.currentTarget, view.id, view.name, event.clientX, event.clientY);
                  }}
                >
                  <span class="tab-name">{view.name}</span>
                  <Show when={props.jobs.marks(view.id).active}>
                    {(count) => (
                      <span
                        class="tab-badge"
                        aria-label={count() === 1 ? "1 job" : `${count()} jobs`}
                        title={count() === 1 ? "1 job running or waiting" : `${count()} jobs running or waiting`}
                      >
                        {count()}
                      </span>
                    )}
                  </Show>
                  <Show when={props.jobs.marks(view.id).unseen}>
                    <span class="tab-marker" aria-label="a job failed" title="A job failed or stopped on a conflict">
                      !
                    </span>
                  </Show>
                </button>
                <button
                  type="button"
                  class="tab-close"
                  aria-label={`Close ${view.name}`}
                  title={`Close ${view.name}`}
                  onClick={() => props.onClose(view.id)}
                >
                  <span aria-hidden="true">×</span>
                </button>
              </div>
            )}
          </For>
        </div>
        <button
          type="button"
          class="tab-add"
          aria-label="Open workspace"
          title="Open or create a workspace"
          aria-haspopup="menu"
          aria-expanded={plusMenu() !== null}
          onClick={(event) => {
            const box = event.currentTarget.getBoundingClientRect();
            setPlusMenu({ right: document.documentElement.clientWidth - box.right, top: box.bottom });
          }}
        >
          +
        </button>
      </div>
      <Show when={error()}>
        {(text) => (
          <div role="alert" class="banner banner-dismissible">
            <span>{text()}</span>
            <button type="button" onClick={() => setError(null)}>
              Dismiss
            </button>
          </div>
        )}
      </Show>
      <Show when={plusMenu()}>
        {(at) => (
          <WorkspaceMenu
            workspaces={props.workspaces}
            dialogs={props.dialogs}
            at={at()}
            onClose={() => setPlusMenu(null)}
            onError={setError}
            onImport={() => props.share.importWorkspace()}
          />
        )}
      </Show>
      <Show when={tabMenu()}>
        {(open) => (
          <div class="menu-backdrop" onClick={() => setTabMenu(null)} onContextMenu={(event) => event.preventDefault()}>
            <div
              class="menu"
              role="menu"
              aria-label="Tab actions"
              style={{ left: `${open().x}px`, top: `${open().y}px` }}
              ref={(element) => queueMicrotask(() => element.querySelector<HTMLElement>("[role=menuitem]")?.focus())}
              onClick={(event) => event.stopPropagation()}
            >
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  const { id, name } = open();
                  setTabMenu(null);
                  props.dialogs.open({ kind: "rename", id, name });
                }}
              >
                Rename…
              </button>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  const { id } = open();
                  setTabMenu(null);
                  props.share.exportWorkspace(id);
                }}
              >
                Export workspace…
              </button>
              <Show when={props.ai.enabled() && views().find((view) => view.id === open().id)}>
                {(view) => (
                  <button
                    type="button"
                    role="menuitemcheckbox"
                    aria-checked={view().ai}
                    disabled={view().read_only}
                    onClick={() => {
                      const { id, ai } = view();
                      setTabMenu(null);
                      send(props.workspaces.setAi(id, !ai));
                    }}
                  >
                    AI for this workspace
                  </button>
                )}
              </Show>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  const { id } = open();
                  setTabMenu(null);
                  props.onClose(id);
                }}
              >
                Close
              </button>
            </div>
          </div>
        )}
      </Show>
    </>
  );
}
