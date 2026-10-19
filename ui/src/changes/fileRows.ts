// The rows of the changes pane's file list (UI-14, UI-15, UI-34): a flat
// list or a folder tree of the files the core listed, in the core's order;
// for a group or several repos, each list is sectioned by repo. This is
// layout only; the lists, their order and their ids come from the core.
import type {
  ChangedFile,
  ChangesContent,
  UnmergedKind,
  FileList,
  FileListMode,
  FileStatus,
  ModeChange,
  RepoChanges,
  RepoId,
  UnmergedFile,
} from "../ipc";
import { strings } from "../strings/en";

/**
 * A file of the view: its list and its id there, and in a merged view its
 * repo. Unmerged files are a list of their own.
 */
export interface FileKey {
  list: FileList | "unmerged";
  id: number;
  repo?: RepoId;
}

export const sameFile = (a: FileKey | null, b: FileKey | null) =>
  a?.list === b?.list && a?.id === b?.id && a?.repo === b?.repo;

type SectionList = "staged" | "unstaged" | "unmerged";

export type FileRow =
  | { kind: "section"; key: string; label: string; count: number; list: SectionList }
  | {
      kind: "repo";
      key: string;
      /** The repo's name, which heads its files in a merged view (UI-34). */
      label: string;
      repo: RepoId;
      list: SectionList;
      count: number;
      /** Why its files cannot be changed here (GRP-8), or `null`. */
      blocked: string | null;
    }
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
      conflict: UnmergedKind | null;
      oldPath: string | null;
      added: number | null;
      removed: number | null;
      modeChange: ModeChange | null;
    };

export type FileEntry = Extract<FileRow, { kind: "file" }>;

const keyOf = (file: FileKey) =>
  file.repo === undefined ? `${file.list}:${file.id}` : `${file.repo}/${file.list}:${file.id}`;

/** The key of `list`, of `repo`'s files in a merged view. */
const listKey = (list: string, repo: RepoId | undefined) => (repo === undefined ? list : `${repo}/${list}`);

/** The key of `list`'s file `id`, of `repo` in a merged view. */
const fileKeyOf = <L extends FileKey["list"]>(list: L, id: number, repo: RepoId | undefined) =>
  repo === undefined ? { list, id } : { list, id, repo };

function changedRow(list: FileList, file: ChangedFile, name: string, depth: number, repo?: RepoId): FileEntry {
  const key = fileKeyOf(list, file.id, repo);
  return {
    kind: "file",
    key: keyOf(key),
    file: key,
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

function unmergedRow(file: UnmergedFile, depth = 0, repo?: RepoId): FileEntry {
  const key = fileKeyOf("unmerged", file.id, repo);
  return {
    kind: "file",
    key: keyOf(key),
    file: key,
    path: file.path,
    name: file.path,
    depth,
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

/**
 * The files of `list` as a folder tree: folders first by name, then files in
 * list order, from depth `base`; in a merged view, of `repo`.
 */
function treeRows(
  list: FileList,
  files: ChangedFile[],
  collapsed: Record<string, boolean>,
  base: number,
  repo: RepoId | undefined,
): FileRow[] {
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
      const key = `${listKey(list, repo)}:dir:${full}`;
      const closed = collapsed[key] === true;
      rows.push({ kind: "folder", key, name: label, depth, collapsed: closed });
      if (!closed) {
        walk(child, full, depth + 1);
      }
    }
    for (const entry of folder.files) {
      rows.push(changedRow(list, entry.file, entry.name, depth, repo));
    }
  };
  walk(root, "", base);
  return rows;
}

function listRows(
  list: FileList,
  files: ChangedFile[],
  mode: FileListMode,
  collapsed: Record<string, boolean>,
  base = 0,
  repo?: RepoId,
): FileRow[] {
  return mode === "tree"
    ? treeRows(list, files, collapsed, base, repo)
    : files.map((file) => changedRow(list, file, file.path, base, repo));
}

const SECTIONS: { list: SectionList; label: string }[] = [
  { list: "unmerged", label: strings.changes.sections.unmerged },
  { list: "staged", label: strings.changes.sections.staged },
  { list: "unstaged", label: strings.changes.sections.unstaged },
];

/**
 * The rows of a merged view (UI-34): Unmerged while any repo has unmerged
 * files, then Staged and Unstaged, each with one section per repo that has
 * files there, in target-set order, its files below its name.
 */
function mergedRows(repos: RepoChanges[], mode: FileListMode, collapsed: Record<string, boolean>): FileRow[] {
  return SECTIONS.flatMap(({ list, label }) => {
    const shown = repos.filter((repo) => repo[list].length > 0);
    const count = shown.reduce((sum, repo) => sum + repo[list].length, 0);
    if (list === "unmerged" && count === 0) {
      return [];
    }
    const section: FileRow = { kind: "section", key: `section:${list}`, label, count, list };
    return [
      section,
      ...shown.flatMap((repo): FileRow[] => [
        {
          kind: "repo",
          key: `repo:${list}:${repo.repo_id}`,
          label: repo.repo_name,
          repo: repo.repo_id,
          list,
          count: repo[list].length,
          blocked: repo.blocked,
        },
        ...(list === "unmerged"
          ? repo.unmerged.map((file) => unmergedRow(file, 1, repo.repo_id))
          : listRows(list, repo[list], mode, collapsed, 1, repo.repo_id)),
      ]),
    ];
  });
}

/**
 * The rows of `content`'s file lists. A commit has one list; uncommitted
 * changes have a Staged and an Unstaged section, and before them, while
 * files are unmerged, an Unmerged section, always by full path (OPS-3). A
 * merged view has the same sections, split by repo.
 */
export function fileRows(content: ChangesContent, mode: FileListMode, collapsed: Record<string, boolean>): FileRow[] {
  if (content.kind === "commit") {
    return listRows("commit", content.files, mode, collapsed);
  }
  if (content.kind === "merged") {
    return mergedRows(content.repos, mode, collapsed);
  }
  const unmerged: FileRow[] =
    content.unmerged.length === 0
      ? []
      : [
          {
            kind: "section",
            key: "section:unmerged",
            label: strings.changes.sections.unmerged,
            count: content.unmerged.length,
            list: "unmerged",
          },
          ...content.unmerged.map((file) => unmergedRow(file)),
        ];
  return [
    ...unmerged,
    {
      kind: "section",
      key: "section:staged",
      label: strings.changes.sections.staged,
      count: content.staged.length,
      list: "staged",
    },
    ...listRows("staged", content.staged, mode, collapsed),
    {
      kind: "section",
      key: "section:unstaged",
      label: strings.changes.sections.unstaged,
      count: content.unstaged.length,
      list: "unstaged",
    },
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
