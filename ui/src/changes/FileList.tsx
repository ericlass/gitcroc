// The changed files (UI-14, UI-15, UI-34), flat or as a folder tree,
// virtualized (PERF-1). Paths are repo text, rendered as text (SEC-3). For
// uncommitted changes each row and each list has stage, unstage and discard
// controls; for a group or several repos each repo's part of a list has
// them, except in a blocked repo (GRP-8). Ctrl/Cmd and Shift select several
// files of one list of one repo, and a double-click moves a file to the
// other list. The palette's file operations run the selected file's
// controls, and Stage all and Unstage all the lists' (UI-22). Which files
// may be changed is the core's to check; this only forwards what was
// clicked (ARCH-6).
import { createVirtualizer } from "@tanstack/solid-virtual";
import { createEffect, createMemo, For, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import type { ChangeAction, ChangesContent, FileListMode, FileStatus, RepoId, UnmergedKind, WorkspaceId } from "../ipc";
import { type Command, type CommandStore, commandOf, type Provider, type Providers } from "../palette/commands";
import { type FileEntry, type FileKey, type FileRow, fileRows, sameFile } from "./fileRows";
import { isMac } from "../keys";
import { strings } from "../strings/en";

/** Height of one row in CSS pixels; `.file-row` in App.css sets it, as `.history-row` does. */
export const FILE_ROW_HEIGHT = 22;

const OVERSCAN = 8;

const STATUS: Record<FileStatus | "unmerged", [letter: string, title: string]> = {
  added: ["A", strings.changes.status.added],
  modified: ["M", strings.changes.status.modified],
  deleted: ["D", strings.changes.status.deleted],
  renamed: ["R", strings.changes.status.renamed],
  type_changed: ["T", strings.changes.status.type_changed],
  untracked: ["U", strings.changes.status.untracked],
  unmerged: ["!", strings.changes.status.unmerged],
};

export const CONFLICTS: Record<UnmergedKind, string> = strings.changes.conflicts;

function FileCells(props: { row: FileEntry; unsaved: boolean }) {
  const status = () => STATUS[props.row.status];
  const title = () => {
    const parts = [`${status()[1]}: ${props.row.path}`];
    if (props.row.oldPath !== null) {
      parts.push(strings.changes.renamedFrom(props.row.oldPath));
    }
    if (props.row.conflict !== null) {
      parts.push(strings.changes.conflictKind(CONFLICTS[props.row.conflict]));
    }
    if (props.row.modeChange !== null) {
      parts.push(strings.changes.modeChange(props.row.modeChange.old, props.row.modeChange.new));
    }
    return parts.join("\n");
  };
  return (
    <>
      <span class={`file-status status-${props.row.status}`} aria-label={status()[1]}>
        {status()[0]}
      </span>
      <span class="file-name" title={title()}>
        {props.row.name}
        <Show when={props.row.oldPath}>{(old) => <span class="file-old"> ← {old()}</span>}</Show>
        <Show when={props.row.conflict}>{(conflict) => <span class="file-conflict"> ({CONFLICTS[conflict()]})</span>}</Show>
        <Show when={props.unsaved}>
          <span
            class="file-unsaved"
            role="img"
            aria-label={strings.changes.unsavedLabel}
            title={strings.changes.unsavedTitle}
          >
            ●
          </span>
        </Show>
      </span>
      <Show when={props.row.added !== null || props.row.removed !== null}>
        <span class="file-counts">
          <span class="added">+{props.row.added ?? 0}</span> <span class="removed">−{props.row.removed ?? 0}</span>
        </span>
      </Show>
    </>
  );
}

/** The controls of a row of `list`, by the action and its label. */
const ROW_ACTIONS: Record<"staged" | "unstaged", { action: ChangeAction; label: string; glyph: string }[]> = {
  staged: [{ action: "unstage", label: strings.changes.unstage, glyph: "−" }],
  unstaged: [
    { action: "discard", label: strings.changes.discard, glyph: "↺" },
    { action: "stage", label: strings.changes.stage, glyph: "+" },
  ],
};

/** What the button that moves all of `list` says. */
const allLabel = (list: "staged" | "unstaged") =>
  list === "staged" ? strings.changes.unstageAll : strings.changes.stageAll;

/** What a double-click, Enter or Space does to a file of `list`. */
const MOVE: Record<"staged" | "unstaged", ChangeAction> = { staged: "unstage", unstaged: "stage" };

export default function FileList(props: {
  content: ChangesContent;
  mode: FileListMode;
  collapsed: Record<string, boolean>;
  selected: FileKey | null;
  /** The files selected together, the selected one among them. */
  marked: FileKey[];
  onSelect: (file: FileKey, marked: FileKey[]) => void;
  onToggleFolder: (key: string) => void;
  /** Stage, unstage or discard files of one list; absent for a commit. */
  onAction?: (action: ChangeAction, files: FileKey[]) => void;
  /** Whether the conflict editor holds unsaved edits of an unmerged file, by path (OPS-9). */
  unsaved?: (path: string) => boolean;
  /** Where the list was scrolled when it was last shown (UI-29). */
  scrollTop: number;
  onScroll: (scrollTop: number) => void;
  /** Where the list provides the palette's file operations, for the workspace it shows (UI-22). */
  commands?: { store: CommandStore; workspace: WorkspaceId };
}) {
  let scroller: HTMLDivElement | undefined;
  const rows = createMemo(() => fileRows(props.content, props.mode, props.collapsed));
  const files = createMemo(() => rows().flatMap((row, index) => (row.kind === "file" ? [index] : [])));
  const selectedIndex = () => rows().findIndex((row) => row.kind === "file" && sameFile(row.file, props.selected));

  const virtualizer = createVirtualizer({
    get count() {
      return rows().length;
    },
    getScrollElement: () => scroller ?? null,
    estimateSize: () => FILE_ROW_HEIGHT,
    overscan: OVERSCAN,
  });

  // Shown again, as when its tab is active again: where it was (UI-29).
  onMount(() => {
    if (scroller) {
      scroller.scrollTop = props.scrollTop;
    }
  });

  const selectAt = (index: number) => {
    const row = rows()[index];
    if (row?.kind === "file") {
      props.onSelect(row.file, [row.file]);
      virtualizer.scrollToIndex(index, { align: "auto" });
    }
  };

  const isMarked = (file: FileKey) => props.marked.some((marked) => sameFile(marked, file));

  /** A click with Ctrl/Cmd adds or removes a file, with Shift selects the run from the selected file. */
  const clickFile = (index: number, event: MouseEvent) => {
    const row = rows()[index];
    if (row?.kind !== "file") {
      return;
    }
    const file = row.file;
    const sameList = (other: FileKey) =>
      other.list === file.list && other.repo === file.repo && file.list !== "unmerged";
    if ((event.ctrlKey || event.metaKey) && sameList(props.selected ?? file)) {
      const marked = isMarked(file)
        ? props.marked.filter((other) => !sameFile(other, file))
        : [...props.marked.filter(sameList), file];
      props.onSelect(marked.at(-1) ?? file, marked.length > 0 ? marked : [file]);
      return;
    }
    const from = selectedIndex();
    if (event.shiftKey && from >= 0 && props.selected !== null && sameList(props.selected)) {
      const [low, high] = from < index ? [from, index] : [index, from];
      const marked = rows()
        .slice(low, high + 1)
        .flatMap((candidate) => (candidate.kind === "file" && sameList(candidate.file) ? [candidate.file] : []));
      props.onSelect(props.selected, marked);
      return;
    }
    selectAt(index);
  };

  /** The files a control of `file`'s row acts on: the selection when the row is in it. */
  const targets = (file: FileKey) =>
    isMarked(file) ? props.marked.filter((other) => other.list === file.list && other.repo === file.repo) : [file];

  /**
   * Files of `list`, of `repo` in a merged view: the list's controls act on
   * all of them. A merged view's lists have them per repo only.
   */
  const listFiles = (list: "staged" | "unstaged", repo?: RepoId): FileKey[] => {
    const content = props.content;
    if (content.kind === "uncommitted" && repo === undefined) {
      return content[list].map((file) => ({ list, id: file.id }));
    }
    if (content.kind === "merged" && repo !== undefined) {
      const files = content.repos.find((shown) => shown.repo_id === repo)?.[list] ?? [];
      return files.map((file) => ({ list, id: file.id, repo }));
    }
    return [];
  };

  /** Why `repo`'s files of a merged view cannot be changed (GRP-8), or `null`. */
  const blocked = (repo: RepoId | undefined) =>
    repo === undefined || props.content.kind !== "merged"
      ? null
      : (props.content.repos.find((shown) => shown.repo_id === repo)?.blocked ?? null);

  const act = (action: ChangeAction, files: FileKey[]) => {
    if (files.length > 0) {
      props.onAction?.(action, files);
    }
  };

  /** A section's list when its files can be staged or unstaged together: not the unmerged files. */
  const changeableList = (list: FileKey["list"]) => (list === "staged" || list === "unstaged" ? list : null);

  /**
   * A row's list when it has stage controls: not a commit's, not an unmerged
   * file (OPS-3 resolves those), not a file of a blocked repo.
   */
  const changeable = (file: FileKey): "staged" | "unstaged" | null =>
    props.onAction !== undefined && (file.list === "staged" || file.list === "unstaged") && blocked(file.repo) === null
      ? file.list
      : null;

  /** `action` on `file` and the files selected with it, as its row's control does it, when the row has it. */
  const fileCommand = (file: FileKey, action: ChangeAction): Command => {
    const list = changeable(file);
    const controls = list === null ? [] : ROW_ACTIONS[list];
    return controls.some((control) => control.action === action)
      ? { run: () => act(action, targets(file)) }
      : commandOf(null, blocked(file.repo) ?? undefined);
  };
  /** Moves `files` of `list` to the other list, as the list's button does. */
  const listCommand = (list: "staged" | "unstaged", files: FileKey[]): Command =>
    files.length > 0 ? { run: () => act(MOVE[list], files) } : commandOf(null);

  /** The palette's `action` on the selected file, as its row's control does it, when the row has it. */
  const onSelected =
    (action: ChangeAction): Provider =>
    () => {
      const file = props.selected;
      if (props.onAction === undefined || file === null) {
        return null;
      }
      return fileCommand(file, action);
    };
  /** The palette's Stage all or Unstage all: the list's button of one repo's changes. */
  const onList =
    (list: "staged" | "unstaged"): Provider =>
    () => {
      if (props.onAction === undefined) {
        return null;
      }
      if (props.content.kind === "merged") {
        return { reason: strings.palette.oneRepo };
      }
      return listCommand(list, listFiles(list));
    };
  const paletteCommands: Providers<"files"> = {
    stage_file: onSelected("stage"),
    unstage_file: onSelected("unstage"),
    discard_file: onSelected("discard"),
    stage_all: onList("unstaged"),
    unstage_all: onList("staged"),
  };
  createEffect(() => {
    const commands = props.commands;
    if (commands !== undefined) {
      onCleanup(commands.store.provide(commands.workspace, "files", paletteCommands));
    }
  });

  const onKeyDown = (event: KeyboardEvent) => {
    // A key on a button in a row, as Stage all, is the button's (UI-22).
    if (event.target !== event.currentTarget) {
      return;
    }
    const at = files().indexOf(selectedIndex());
    const go = (position: number) => selectAt(files()[Math.max(0, Math.min(files().length - 1, position))] ?? -1);
    switch (event.key) {
      case "ArrowDown":
        go(at + 1);
        break;
      case "ArrowUp":
        go(at < 0 ? 0 : at - 1);
        break;
      case "Home":
        go(0);
        break;
      case "End":
        go(files().length - 1);
        break;
      case "Enter":
      case " ": {
        const list = props.selected === null ? null : changeable(props.selected);
        if (list === null || props.selected === null) {
          return;
        }
        act(MOVE[list], targets(props.selected));
        break;
      }
      // Delete asks to discard the file (GRP-9); on macOS the key marked delete is Backspace.
      case "Backspace":
      case "Delete": {
        if (event.key === "Backspace" && !isMac()) {
          return;
        }
        const list = props.selected === null ? null : changeable(props.selected);
        if (list !== "unstaged" || props.selected === null) {
          return;
        }
        act("discard", targets(props.selected));
        break;
      }
      default:
        return;
    }
    event.preventDefault();
  };

  const rowView = (row: FileRow) => (
    <Switch>
      <Match when={row.kind === "section" && row}>
        {(section) => (
          <>
            <span class="file-section">
              {section().label} ({section().count})
            </span>
            <Show when={props.onAction !== undefined && changeableList(section().list)}>
              {(list) => (
                <Show when={listFiles(list()).length > 0}>
                  <button
                    type="button"
                    class="file-list-action"
                    onClick={(event) => {
                      event.stopPropagation();
                      act(MOVE[list()], listFiles(list()));
                    }}
                  >
                    {allLabel(list())}
                  </button>
                </Show>
              )}
            </Show>
          </>
        )}
      </Match>
      <Match when={row.kind === "repo" && row}>
        {(repo) => (
          <>
            <span class="file-repo" title={repo().label}>
              {repo().label} ({repo().count})
            </span>
            <Show
              when={repo().blocked === null}
              fallback={
                <span class="file-blocked" title={repo().blocked ?? ""}>
                  {strings.changes.blocked}
                </span>
              }
            >
              <Show when={props.onAction !== undefined && changeableList(repo().list)}>
                {(list) => (
                  <button
                    type="button"
                    class="file-list-action"
                    aria-label={strings.changes.allIn(allLabel(list()), repo().label)}
                    onClick={(event) => {
                      event.stopPropagation();
                      act(MOVE[list()], listFiles(list(), repo().repo));
                    }}
                  >
                    {allLabel(list())}
                  </button>
                )}
              </Show>
            </Show>
          </>
        )}
      </Match>
      <Match when={row.kind === "folder" && row}>
        {(folder) => (
          <span class="file-folder" title={folder().name}>
            <span class="file-chevron" aria-hidden="true">
              {folder().collapsed ? "▸" : "▾"}
            </span>
            {folder().name}
          </span>
        )}
      </Match>
      <Match when={row.kind === "file" && row}>
        {(file) => (
          <>
            <FileCells row={file()} unsaved={file().file.list === "unmerged" && (props.unsaved?.(file().path) ?? false)} />
            <Show when={changeable(file().file)}>
              {(list) => (
                <span class="file-actions">
                  <For each={ROW_ACTIONS[list()]}>
                    {(control) => (
                      <button
                        type="button"
                        tabindex="-1"
                        aria-label={`${control.label} ${file().path}`}
                        title={control.label}
                        onClick={(event) => {
                          event.stopPropagation();
                          act(control.action, targets(file().file));
                        }}
                        onDblClick={(event) => event.stopPropagation()}
                      >
                        {control.glyph}
                      </button>
                    )}
                  </For>
                </span>
              )}
            </Show>
          </>
        )}
      </Match>
    </Switch>
  );

  return (
    <div
      class="file-list"
      ref={(element) => (scroller = element)}
      role="listbox"
      aria-label={strings.changes.changedFiles}
      aria-multiselectable={props.onAction !== undefined ? "true" : undefined}
      tabindex="0"
      aria-activedescendant={selectedIndex() >= 0 ? `file-row-${selectedIndex()}` : undefined}
      onKeyDown={onKeyDown}
      onScroll={(event) => props.onScroll(event.currentTarget.scrollTop)}
    >
      <div class="file-rows" style={{ height: `${virtualizer.getTotalSize()}px` }}>
        <For each={virtualizer.getVirtualItems()}>
          {(item) => (
            <Show when={rows()[item.index]}>
              {(row) => {
                const depth = () => {
                  const current = row();
                  return current.kind === "section" || current.kind === "repo" ? 0 : current.depth;
                };
                const file = () => {
                  const current = row();
                  return current.kind === "file" ? current : null;
                };
                return (
                  <div
                    id={`file-row-${item.index}`}
                    class={`file-row file-row-${row().kind}`}
                    classList={{
                      selected: sameFile(file()?.file ?? null, props.selected) && file() !== null,
                      marked: (() => {
                        const current = file();
                        return current !== null && props.marked.length > 1 && isMarked(current.file);
                      })(),
                    }}
                    role={row().kind === "file" ? "option" : "presentation"}
                    aria-selected={(() => {
                      const current = file();
                      return current === null ? undefined : sameFile(current.file, props.selected) || isMarked(current.file);
                    })()}
                    aria-expanded={(() => {
                      const current = row();
                      return current.kind === "folder" ? !current.collapsed : undefined;
                    })()}
                    data-index={item.index}
                    style={{
                      transform: `translateY(${item.start}px)`,
                      "padding-left": `${8 + depth() * 14}px`,
                    }}
                    onClick={(event) => {
                      const current = row();
                      if (current.kind === "folder") {
                        props.onToggleFolder(current.key);
                      } else {
                        clickFile(item.index, event);
                      }
                    }}
                    onDblClick={() => {
                      const current = row();
                      const list = current.kind === "file" ? changeable(current.file) : null;
                      if (current.kind === "file" && list !== null) {
                        act(MOVE[list], [current.file]);
                      }
                    }}
                  >
                    {rowView(row())}
                  </div>
                );
              }}
            </Show>
          )}
        </For>
      </div>
    </div>
  );
}
