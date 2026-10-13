// The rows of the changes pane's file list (UI-14, UI-15): a flat list or
// a folder tree of the files the core listed, in the core's order. This is
// layout only; the lists, their order and their ids come from the core.
import type {
  ChangedFile,
  ChangesContent,
  Conflict,
  FileList,
  FileListMode,
  FileStatus,
  ModeChange,
  UnmergedFile,
} from "../ipc";

/** A file of the view: its list and its id there. Unmerged files are a list of their own. */
export interface FileKey {
  list: FileList | "unmerged";
  id: number;
}

export const sameFile = (a: FileKey | null, b: FileKey | null) => a?.list === b?.list && a?.id === b?.id;

export type FileRow =
  | { kind: "section"; key: string; label: string; count: number }
  | { kind: "folder"; key: string; name: string; depth: number; collapsed: boolean }
  | {
      kind: "file";
      key: string;
      file: FileKey;
      /** The full path, for the tooltip. */
      path: string;
      /** What the row shows: the full path in a flat list, the name in a tree. */
      name: string;
      depth: number;
      status: FileStatus | "unmerged";
      conflict: Conflict | null;
      oldPath: string | null;
      added: number | null;
      removed: number | null;
      modeChange: ModeChange | null;
    };

export type FileEntry = Extract<FileRow, { kind: "file" }>;

const keyOf = (file: FileKey) => `${file.list}:${file.id}`;

function changedRow(list: FileList, file: ChangedFile, name: string, depth: number): FileEntry {
  return {
    kind: "file",
    key: keyOf({ list, id: file.id }),
    file: { list, id: file.id },
    path: file.path,
    name,
    depth,
    status: file.status,
    conflict: null,
    oldPath: file.old_path,
    added: file.added,
    removed: file.removed,
    modeChange: file.mode_change,
  };
}

function unmergedRow(file: UnmergedFile): FileEntry {
  return {
    kind: "file",
    key: keyOf({ list: "unmerged", id: file.id }),
    file: { list: "unmerged", id: file.id },
    path: file.path,
    name: file.path,
    depth: 0,
    status: "unmerged",
    conflict: file.conflict,
    oldPath: null,
    added: null,
    removed: null,
    modeChange: null,
  };
}

interface Folder {
  folders: Map<string, Folder>;
  files: { name: string; file: ChangedFile }[];
}

const emptyFolder = (): Folder => ({ folders: new Map(), files: [] });

/** The files of `list` as a folder tree: folders first by name, then files in list order. */
function treeRows(list: FileList, files: ChangedFile[], collapsed: Record<string, boolean>): FileRow[] {
  const root = emptyFolder();
  for (const file of files) {
    // A folder git lists as one entry ends in `/`; it stays one row.
    const directory = file.path.endsWith("/");
    const parts = (directory ? file.path.slice(0, -1) : file.path).split("/");
    const name = `${parts.pop() ?? file.path}${directory ? "/" : ""}`;
    let folder = root;
    for (const part of parts) {
      let next = folder.folders.get(part);
      if (!next) {
        next = emptyFolder();
        folder.folders.set(part, next);
      }
      folder = next;
    }
    folder.files.push({ name, file });
  }
  const rows: FileRow[] = [];
  const walk = (folder: Folder, path: string, depth: number) => {
    for (const name of [...folder.folders.keys()].sort()) {
      let child = folder.folders.get(name) ?? emptyFolder();
      // A folder whose only child is a folder shows as one row: `src/util`.
      let label = name;
      while (child.files.length === 0 && child.folders.size === 1) {
        const [[inner, only]] = [...child.folders.entries()] as [[string, Folder]];
        label = `${label}/${inner}`;
        child = only;
      }
      const full = `${path}${label}/`;
      const key = `${list}:dir:${full}`;
      const closed = collapsed[key] === true;
      rows.push({ kind: "folder", key, name: label, depth, collapsed: closed });
      if (!closed) {
        walk(child, full, depth + 1);
      }
    }
    for (const entry of folder.files) {
      rows.push(changedRow(list, entry.file, entry.name, depth));
    }
  };
  walk(root, "", 0);
  return rows;
}

function listRows(
  list: FileList,
  files: ChangedFile[],
  mode: FileListMode,
  collapsed: Record<string, boolean>,
): FileRow[] {
  return mode === "tree"
    ? treeRows(list, files, collapsed)
    : files.map((file) => changedRow(list, file, file.path, 0));
}

/**
 * The rows of `content`'s file lists. A commit has one list; uncommitted
 * changes have a Staged and an Unstaged section, the unmerged files first
 * in Unstaged, always by full path.
 */
export function fileRows(content: ChangesContent, mode: FileListMode, collapsed: Record<string, boolean>): FileRow[] {
  if (content.kind === "commit") {
    return listRows("commit", content.files, mode, collapsed);
  }
  const unstaged = content.unstaged.length + content.unmerged.length;
  return [
    { kind: "section", key: "section:staged", label: "Staged", count: content.staged.length },
    ...listRows("staged", content.staged, mode, collapsed),
    { kind: "section", key: "section:unstaged", label: "Unstaged", count: unstaged },
    ...content.unmerged.map(unmergedRow),
    ...listRows("unstaged", content.unstaged, mode, collapsed),
  ];
}

/** The first file the list shows, collapsed folders aside; it is selected automatically. */
export function firstFile(content: ChangesContent, mode: FileListMode): FileKey | null {
  const first = fileRows(content, mode, {}).find((row) => row.kind === "file");
  return first?.kind === "file" ? first.file : null;
}

/** The entry of `file` in `content`, by full path; `null` when it is not there. */
export function entryOf(content: ChangesContent, file: FileKey): FileEntry | null {
  const row = fileRows(content, "flat", {}).find((candidate) => candidate.kind === "file" && sameFile(candidate.file, file));
  return row?.kind === "file" ? row : null;
}
