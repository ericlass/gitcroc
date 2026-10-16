// Export and import of workspaces (DATA-2): asks the core to open its save
// or open dialog (SEC-2), shows how it went, and opens the missing repos
// dialog after an import that has missing repos, or for one repo from its
// context menu. The core writes, reads, resolves and checks; this store
// only sends requests and keeps the answers.
import { createStore, produce } from "solid-js/store";
import { core, type RepoId, type WorkspaceId, workspaceExport, workspaceImport } from "../ipc";

/** The missing repos dialog of a workspace: the repo it was opened for, checked alone, or `null` for all. */
export interface MissingOpen {
  focus: RepoId | null;
}

/** What the last export or import said. */
export interface ShareNotice {
  text: string;
  error: boolean;
}

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** `text` with a capital first letter, as the core's messages start in lower case. */
const sentence = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

export function createShareStore() {
  const [state, setState] = createStore<{
    notice: ShareNotice | null;
    missing: Record<WorkspaceId, MissingOpen | undefined>;
  }>({ notice: null, missing: {} });

  const say = (text: string, error: boolean) => setState("notice", { text, error });

  const dismiss = () => setState("notice", null);

  /** Asks the core for the save dialog; the result arrives as `workspace_exported`. */
  const exportWorkspace = (id: WorkspaceId) => {
    dismiss();
    workspaceExport(id).catch((failure) => say(`Cannot export the workspace: ${message(failure)}`, true));
  };

  /** Asks the core for the open dialog; the result arrives as `workspace_imported`. */
  const importWorkspace = () => {
    dismiss();
    workspaceImport().catch((failure) => say(`Cannot import a workspace: ${message(failure)}`, true));
  };

  const openMissing = (id: WorkspaceId, focus: RepoId | null) =>
    setState(
      produce((draft) => {
        draft.missing[id] = { focus };
      }),
    );

  const closeMissing = (id: WorkspaceId) =>
    setState(
      produce((draft) => {
        draft.missing[id] = undefined;
      }),
    );

  const missingOpen = (id: WorkspaceId) => state.missing[id];

  /** Follows the ends of exports and imports; resolves to the unsubscribe function. */
  const subscribe = async () => {
    const stopExported = await core.events.workspaceExported.listen((event) => {
      const { report, error } = event.payload;
      if (report === null) {
        say(sentence(error ?? "the export failed"), true);
        return;
      }
      const absolute =
        report.absolute.length === 0
          ? ""
          : ` No relative path reaches these repos, so they stay absolute: ${report.absolute.join(", ")}.`;
      say(`Exported the workspace to ${report.path}.${absolute}`, report.absolute.length > 0);
    });
    const stopImported = await core.events.workspaceImported.listen((event) => {
      const { report, error } = event.payload;
      if (report === null) {
        say(sentence(error ?? "the import failed"), true);
      } else if (report.missing.length > 0) {
        openMissing(report.workspace_id, null);
      }
    });
    return () => {
      stopExported();
      stopImported();
    };
  };

  return { state, dismiss, exportWorkspace, importWorkspace, openMissing, closeMissing, missingOpen, subscribe };
}

export type ShareStore = ReturnType<typeof createShareStore>;
