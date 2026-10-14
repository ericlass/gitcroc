// The changed files (UI-14, UI-15), flat or as a folder tree, virtualized
// (PERF-1). Paths are repo text, rendered as text (SEC-3). For uncommitted
// changes each row and each list has stage, unstage and discard controls;
// Ctrl/Cmd and Shift select several files of one list, and a double-click
// moves a file to the other list. Which files may be changed is the
// core's to check; this only forwards what was clicked (ARCH-6).
import { createVirtualizer } from "@tanstack/solid-virtual";
import { createMemo, For, Match, Show, Switch } from "solid-js";
import type { ChangeAction, ChangesContent, FileListMode, FileStatus, UnmergedKind } from "../ipc";
import { type FileEntry, type FileKey, type FileRow, fileRows, sameFile } from "./fileRows";

/** Height of one row in CSS pixels; `.file-row` in App.css sets it, as `.history-row` does. */
export const FILE_ROW_HEIGHT = 22;

const OVERSCAN = 8;

const STATUS: Record<FileStatus | "unmerged", [letter: string, title: string]> = {
  added: ["A", "Added"],
  modified: ["M", "Modified"],
  deleted: ["D", "Deleted"],
  renamed: ["R", "Renamed"],
  type_changed: ["T", "Type changed"],
  untracked: ["U", "Untracked"],
  unmerged: ["!", "Conflict"],
};

export const CONFLICTS: Record<UnmergedKind, string> = {
  both_deleted: "both deleted",
  added_by_us: "added by us",
  deleted_by_them: "deleted by them",
  added_by_them: "added by them",
  deleted_by_us: "deleted by us",
  both_added: "both added",
  both_modified: "both modified",
};

function FileCells(props: { row: FileEntry }) {
  const status = () => STATUS[props.row.status];
  const title = () => {
    const parts = [`${status()[1]}: ${props.row.path}`];
    if (props.row.oldPath !== null) {
      parts.push(`Renamed from ${props.row.oldPath}`);
    }
    if (props.row.conflict !== null) {
      parts.push(`Conflict: ${CONFLICTS[props.row.conflict]}`);
    }
    if (props.row.modeChange !== null) {
      parts.push(`Mode ${props.row.modeChange.old} → ${props.row.modeChange.new}`);
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
  staged: [{ action: "unstage", label: "Unstage", glyph: "−" }],
  unstaged: [
    { action: "discard", label: "Discard", glyph: "↺" },
    { action: "stage", label: "Stage", glyph: "+" },
  ],
};

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
    const sameList = (other: FileKey) => other.list === file.list && file.list !== "unmerged";
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
  const targets = (file: FileKey) => (isMarked(file) ? props.marked.filter((other) => other.list === file.list) : [file]);

  /** Files of `list`: the list's controls act on all of them. */
  const listFiles = (list: "staged" | "unstaged"): FileKey[] =>
    props.content.kind === "uncommitted"
      ? props.content[list].map((file) => ({ list, id: file.id }))
      : [];

  const act = (action: ChangeAction, files: FileKey[]) => {
    if (files.length > 0) {
      props.onAction?.(action, files);
    }
  };

  /** A row's list when it has stage controls: not a commit's, not an unmerged file (M2-10). */
  const changeable = (file: FileKey): "staged" | "unstaged" | null =>
    props.onAction !== undefined && (file.list === "staged" || file.list === "unstaged") ? file.list : null;

  const onKeyDown = (event: KeyboardEvent) => {
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
      case "Delete": {
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
            <Show when={props.onAction !== undefined && listFiles(section().list).length > 0}>
              <button
                type="button"
                class="file-list-action"
                onClick={(event) => {
                  event.stopPropagation();
                  act(MOVE[section().list], listFiles(section().list));
                }}
              >
                {section().list === "staged" ? "Unstage all" : "Stage all"}
              </button>
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
            <FileCells row={file()} />
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
      aria-label="Changed files"
      aria-multiselectable={props.onAction !== undefined ? "true" : undefined}
      tabindex="0"
      aria-activedescendant={selectedIndex() >= 0 ? `file-row-${selectedIndex()}` : undefined}
      onKeyDown={onKeyDown}
    >
      <div class="file-rows" style={{ height: `${virtualizer.getTotalSize()}px` }}>
        <For each={virtualizer.getVirtualItems()}>
          {(item) => (
            <Show when={rows()[item.index]}>
              {(row) => {
                const depth = () => {
                  const current = row();
                  return current.kind === "section" ? 0 : current.depth;
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
