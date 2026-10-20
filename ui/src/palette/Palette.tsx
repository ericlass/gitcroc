// The command palette (UI-22): a text box above the core's answer to it,
// virtualized (PERF-1). Each change of the text asks the core again; the
// list keeps the last answer until a newer one arrives, so typing never
// waits, and an answer the core superseded is dropped. The keys move the
// highlight and Enter runs its row: an operation runs what its place in the
// window provides, a repo, group or branch is handed to the window to
// select. An operation that cannot apply now says why and does nothing.
// The core finds, ranks and labels the rows (ARCH-6); names are text only
// (SEC-3).
import { createVirtualizer } from "@tanstack/solid-virtual";
import { createEffect, createSignal, For, on, Show } from "solid-js";
import { Frame } from "../dialogs/parts";
import { paletteQuery, type PaletteItem, type PaletteResult, type WorkspaceId } from "../ipc";
import { PALETTE_SHORTCUTS, shortcutLabel } from "../keys";
import { strings } from "../strings/en";
import { type Command, type CommandStore, missingSubject } from "./commands";

/** Height of one row in CSS pixels; `.palette-row` in App.css sets it. */
export const PALETTE_ROW_HEIGHT = 28;

/** Rows rendered beyond each edge of the visible ones. */
export const PALETTE_OVERSCAN = 6;

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** What a row's kind is called. */
function kindOf(item: PaletteItem): string {
  const kinds = strings.palette.kinds;
  switch (item.kind) {
    case "operation":
      return kinds.operation;
    case "repo":
      return kinds.repo;
    case "group":
      return kinds.group;
    case "branch":
      return item.remote === null ? kinds.branch : kinds.remoteBranch;
  }
}

/** A row's text: an operation's label, or a repo's, group's or branch's name. */
const textOf = (item: PaletteItem) => (item.kind === "operation" ? item.label : item.name);

/** What a row says after its name: a repo's branch, a group's size, a branch's repo. */
function detailOf(item: PaletteItem): string | null {
  switch (item.kind) {
    case "operation":
      return null;
    case "repo":
      return item.branch;
    case "group":
      return strings.palette.members(item.members);
    case "branch":
      return strings.palette.inRepo(item.repo_name);
  }
}

export default function Palette(props: {
  workspace: WorkspaceId;
  commands: CommandStore;
  /** Closes the palette; called before a chosen row runs. */
  onClose: () => void;
  /** Selects a repo, group or branch row's item in the tree. */
  onSelect: (item: Exclude<PaletteItem, { kind: "operation" }>) => void;
}) {
  let scroller: HTMLDivElement | undefined;
  const [text, setText] = createSignal("");
  const [result, setResult] = createSignal<PaletteResult | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [highlight, setHighlight] = createSignal(0);
  const items = () => result()?.items ?? [];
  /** The newest request sent, and the newest one whose answer is shown. */
  let sent = 0;
  let shown = 0;

  const virtualizer = createVirtualizer({
    get count() {
      return items().length;
    },
    getScrollElement: () => scroller ?? null,
    estimateSize: () => PALETTE_ROW_HEIGHT,
    overscan: PALETTE_OVERSCAN,
  });

  createEffect(
    on(text, (query) => {
      const mine = ++sent;
      paletteQuery(props.workspace, query).then(
        (answer) => {
          // `null`: the core superseded it. An older answer never replaces a newer one.
          if (answer !== null && mine > shown) {
            shown = mine;
            setResult(answer);
            setError(null);
            setHighlight(0);
            virtualizer.scrollToIndex(0);
          }
        },
        (failure: unknown) => {
          if (mine === sent) {
            setError(message(failure));
          }
        },
      );
    }),
  );

  /** What an operation row does now; other rows always run. */
  const commandOf = (item: PaletteItem): Command | null =>
    item.kind === "operation"
      ? (props.commands.command(props.workspace, item.op) ?? { reason: missingSubject(item.subject) })
      : null;

  const reasonOf = (item: PaletteItem) => {
    const command = commandOf(item);
    return command !== null && "reason" in command ? command.reason : null;
  };

  const run = (index: number) => {
    const item = items()[index];
    if (item === undefined) {
      return;
    }
    if (item.kind !== "operation") {
      props.onClose();
      props.onSelect(item);
      return;
    }
    const command = commandOf(item);
    if (command !== null && "run" in command) {
      props.onClose();
      command.run();
    }
  };

  const move = (index: number) => {
    const last = items().length - 1;
    if (last < 0) {
      return;
    }
    const next = Math.max(0, Math.min(last, index));
    setHighlight(next);
    virtualizer.scrollToIndex(next, { align: "auto" });
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const height = scroller?.getBoundingClientRect().height ?? PALETTE_ROW_HEIGHT;
    const page = Math.max(1, Math.floor(height / PALETTE_ROW_HEIGHT) - 1);
    switch (event.key) {
      case "ArrowDown":
        move(highlight() + 1);
        break;
      case "ArrowUp":
        move(highlight() - 1);
        break;
      case "PageDown":
        move(highlight() + page);
        break;
      case "PageUp":
        move(highlight() - page);
        break;
      case "Home":
        move(0);
        break;
      case "End":
        move(items().length - 1);
        break;
      case "Enter":
        run(highlight());
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  return (
    <Frame id="palette" title={strings.palette.label} class="palette" onClose={() => props.onClose()}>
      <input
        type="text"
        class="palette-input"
        role="combobox"
        aria-label={strings.palette.label}
        aria-expanded="true"
        aria-controls="palette-list"
        aria-autocomplete="list"
        aria-activedescendant={items().length > 0 ? `palette-row-${highlight()}` : undefined}
        placeholder={strings.palette.search}
        autocomplete="off"
        spellcheck={false}
        value={text()}
        onInput={(event) => setText(event.currentTarget.value)}
        onKeyDown={onKeyDown}
      />
      <Show when={error()}>{(failure) => <p role="alert">{strings.palette.cannotRead(failure())}</p>}</Show>
      <div
        id="palette-list"
        class="palette-list"
        role="listbox"
        aria-label={strings.palette.results}
        ref={(element) => (scroller = element)}
      >
        <div class="palette-rows" style={{ height: `${virtualizer.getTotalSize()}px` }}>
          <For each={virtualizer.getVirtualItems()}>
            {(row) => (
              <Show when={items()[row.index]}>
                {(item) => {
                  const shortcut = () => {
                    const shown = item();
                    const id = shown.kind === "operation" ? PALETTE_SHORTCUTS[shown.op] : undefined;
                    return id === undefined ? undefined : shortcutLabel(id);
                  };
                  return (
                    <div
                      id={`palette-row-${row.index}`}
                      class="palette-row"
                      classList={{ highlighted: row.index === highlight(), disabled: reasonOf(item()) !== null }}
                      role="option"
                      aria-selected={row.index === highlight()}
                      aria-disabled={reasonOf(item()) !== null ? true : undefined}
                      data-index={row.index}
                      title={reasonOf(item()) ?? undefined}
                      style={{ transform: `translateY(${row.start}px)` }}
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => {
                        setHighlight(row.index);
                        run(row.index);
                      }}
                    >
                      <span class="palette-kind">{kindOf(item())}</span>
                      <span class="palette-text">
                        {textOf(item())}
                      </span>
                      <Show when={detailOf(item())}>{(detail) => <span class="palette-detail">{detail()}</span>}</Show>
                      <Show when={reasonOf(item())}>{(reason) => <span class="palette-reason">{reason()}</span>}</Show>
                      <Show when={shortcut()}>{(keys) => <kbd class="palette-shortcut">{keys()}</kbd>}</Show>
                    </div>
                  );
                }}
              </Show>
            )}
          </For>
        </div>
      </div>
      <Show when={result()}>
        {(answer) => (
          <Show
            when={answer().total > answer().items.length}
            fallback={
              <Show when={answer().total === 0}>
                <p class="palette-more">{strings.palette.nothing}</p>
              </Show>
            }
          >
            <p class="palette-more">{strings.palette.more(answer().items.length, answer().total)}</p>
          </Show>
        )}
      </Show>
    </Frame>
  );
}
