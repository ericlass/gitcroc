// The changed files (UI-14, UI-15), flat or as a folder tree, virtualized
// (PERF-1). Paths are repo text, rendered as text (SEC-3).
import { createVirtualizer } from "@tanstack/solid-virtual";
import { createMemo, For, Match, Show, Switch } from "solid-js";
import type { ChangesContent, Conflict, FileListMode, FileStatus } from "../ipc";
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

export const CONFLICTS: Record<Conflict, string> = {
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

export default function FileList(props: {
  content: ChangesContent;
  mode: FileListMode;
  collapsed: Record<string, boolean>;
  selected: FileKey | null;
  onSelect: (file: FileKey) => void;
  onToggleFolder: (key: string) => void;
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
      props.onSelect(row.file);
      virtualizer.scrollToIndex(index, { align: "auto" });
    }
  };

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
      default:
        return;
    }
    event.preventDefault();
  };

  const rowView = (row: FileRow) => (
    <Switch>
      <Match when={row.kind === "section" && row}>
        {(section) => (
          <span class="file-section">
            {section().label} ({section().count})
          </span>
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
      <Match when={row.kind === "file" && row}>{(file) => <FileCells row={file()} />}</Match>
    </Switch>
  );

  return (
    <div
      class="file-list"
      ref={(element) => (scroller = element)}
      role="listbox"
      aria-label="Changed files"
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
                    classList={{ selected: sameFile(file()?.file ?? null, props.selected) && file() !== null }}
                    role={row().kind === "file" ? "option" : "presentation"}
                    aria-selected={row().kind === "file" ? sameFile(file()?.file ?? null, props.selected) : undefined}
                    aria-expanded={(() => {
                      const current = row();
                      return current.kind === "folder" ? !current.collapsed : undefined;
                    })()}
                    data-index={item.index}
                    style={{
                      transform: `translateY(${item.start}px)`,
                      "padding-left": `${8 + depth() * 14}px`,
                    }}
                    onClick={() => {
                      const current = row();
                      if (current.kind === "folder") {
                        props.onToggleFolder(current.key);
                      } else {
                        selectAt(item.index);
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
