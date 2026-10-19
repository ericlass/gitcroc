// The missing repos dialog (DATA-2): after an import, or from a missing
// repo's context menu, it lists the workspace's repos whose folder is
// missing, with their remote. Each row has a check box, checked when the
// repo has a remote (or only the repo the dialog was opened for), and its
// target as parent folder plus name, which the core checks as the user
// edits it; a parent that does not exist yet is created by the clone.
// Clone selected starts one clone job per checked row, all at once; a row
// then shows how its clone ended, with git's message when it failed. Names,
// paths and URLs come from a shared file and are shown as text only (SEC-3).
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { Frame, Refusal } from "../dialogs/parts";
import {
  type CloneParentPickedEvent,
  type CloneReport,
  clonePickParent,
  cloneStart,
  cloneTargetCheck,
  core,
  type JobId,
  type MissingRepo,
  type WorkspaceId,
  workspaceMissingRepos,
} from "../ipc";
import type { ShareStore } from "../stores/share";
import { strings } from "../strings/en";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

interface Row {
  repo: MissingRepo;
  checked: boolean;
  parent: string;
  name: string;
  /** The core's verdict on the target `key` (parent and name). */
  check: { key: string; error: string | null } | null;
  state: "idle" | "cloning" | "cloned" | "failed";
  job: JobId | null;
  /** Why the clone failed. */
  message: string | null;
}

/** The key of a row's folder picks. */
const pickKey = (row: Row) => `missing:${row.repo.repo_id}`;
const targetKey = (row: Row) => `${row.parent}\n${row.name}`;

export default function MissingReposDialog(props: { workspace: WorkspaceId; share: ShareStore }) {
  const [rows, setRows] = createStore<Row[]>([]);
  const [loaded, setLoaded] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  /** Reports that arrived before their job's id did. */
  const early = new Map<JobId, CloneReport>();
  const close = () => props.share.closeMissing(props.workspace);

  const change = (index: number, edit: (row: Row) => void) =>
    setRows(
      produce((draft) => {
        const row = draft[index];
        if (row !== undefined) {
          edit(row);
        }
      }),
    );

  /** Asks the core about row `index`'s target; only the latest answer counts. */
  const recheck = (index: number) => {
    const row = rows[index];
    if (row === undefined) {
      return;
    }
    const key = targetKey(row);
    const keep = (answer: string | null) => {
      const now = rows[index];
      if (now !== undefined && targetKey(now) === key) {
        change(index, (draft) => (draft.check = { key, error: answer }));
      }
    };
    cloneTargetCheck(row.parent, row.name, true)
      .then(keep)
      .catch((failure) => keep(message(failure)));
  };

  /** Row `index`'s clone ended as `report` said. */
  const ended = (index: number, report: CloneReport) =>
    change(index, (draft) => {
      draft.state = report.repo === null ? "failed" : "cloned";
      draft.message = report.repo === null ? (report.message ?? strings.clone.missing.cloneFailed) : null;
      draft.checked = report.repo === null && draft.checked;
    });

  /** A folder picked for a row's target. */
  const onPicked = (event: { payload: CloneParentPickedEvent }) => {
    const { workspace_id, path, key } = event.payload;
    const index = rows.findIndex((row) => pickKey(row) === key);
    if (workspace_id === props.workspace && index >= 0) {
      change(index, (draft) => (draft.parent = path));
      recheck(index);
    }
  };

  /** A clone ended: its row's, or one whose job id is still on its way. */
  const onReport = (event: { payload: CloneReport }) => {
    const index = rows.findIndex((row) => row.job === event.payload.job_id);
    if (index >= 0) {
      ended(index, event.payload);
    } else if (event.payload.workspace_id === props.workspace) {
      early.set(event.payload.job_id, event.payload);
    }
  };

  onMount(() => {
    const focus = props.share.missingOpen(props.workspace)?.focus ?? null;
    workspaceMissingRepos(props.workspace)
      .then((missing) => {
        setRows(
          missing.map((repo) => ({
            repo,
            checked: repo.remote !== null && (focus === null || focus === repo.repo_id),
            parent: repo.parent,
            name: repo.folder,
            check: null,
            state: "idle",
            job: null,
            message: null,
          })),
        );
        setLoaded(true);
        missing.forEach((_, index) => recheck(index));
      })
      .catch((failure) => setError(message(failure)));
    const listening = [core.events.cloneParentPicked.listen(onPicked), core.events.cloneReport.listen(onReport)];
    onCleanup(() => {
      for (const pending of listening) {
        void pending.then((stop) => stop());
      }
    });
  });

  /** The target was checked as it is now and the core had nothing against it. */
  const targetOk = (row: Row) => row.check?.key === targetKey(row) && row.check.error === null;
  const clonable = (row: Row) =>
    row.checked && row.repo.remote !== null && row.state !== "cloning" && row.state !== "cloned" && targetOk(row);

  const cloneSelected = () => {
    setError(null);
    rows.forEach((row, index) => {
      if (!clonable(row) || row.repo.remote === null) {
        return;
      }
      change(index, (draft) => {
        draft.state = "cloning";
        draft.message = null;
      });
      const request = {
        url: row.repo.remote,
        parent: row.parent,
        name: row.name,
        repo: row.repo.repo_id,
        group: null,
      };
      cloneStart(props.workspace, request)
        .then((job) => {
          change(index, (draft) => (draft.job = job));
          const report = early.get(job);
          if (report !== undefined) {
            early.delete(job);
            ended(index, report);
          }
        })
        .catch((failure) =>
          change(index, (draft) => {
            draft.state = "failed";
            draft.message = message(failure);
          }),
        );
    });
  };

  return (
    <Frame id="missing-repos" title={strings.clone.missing.title} class="missing-dialog" onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          cloneSelected();
        }}
      >
        <Show when={loaded()} fallback={<p>{strings.clone.missing.looking}</p>}>
          <Show when={rows.length > 0} fallback={<p>{strings.clone.missing.none}</p>}>
            <p>{strings.clone.missing.intro}</p>
            <ul class="missing-list" aria-label={strings.clone.missing.title}>
              <For each={rows}>
                {(row, index) => (
                  <li class="missing-row">
                    <label class="missing-check">
                      <input
                        type="checkbox"
                        checked={row.checked}
                        disabled={row.repo.remote === null || row.state === "cloning" || row.state === "cloned"}
                        onChange={(event) => {
                          const checked = event.currentTarget.checked;
                          change(index(), (draft) => (draft.checked = checked));
                        }}
                      />
                      <span class="missing-name">{row.repo.name}</span>
                    </label>
                    <span class="missing-remote">{row.repo.remote ?? strings.clone.missing.noRemote}</span>
                    <div class="missing-target">
                      <span class="clone-path" title={row.parent}>
                        {row.parent}
                      </span>
                      <button
                        type="button"
                        aria-label={strings.clone.missing.chooseFolder(row.repo.name)}
                        disabled={row.state === "cloning" || row.state === "cloned"}
                        onClick={() => {
                          clonePickParent(props.workspace, pickKey(row)).catch((failure) => setError(message(failure)));
                        }}
                      >
                        {strings.clone.choose}
                      </button>
                      <input
                        type="text"
                        autocomplete="off"
                        spellcheck={false}
                        aria-label={strings.clone.missing.folderName(row.repo.name)}
                        value={row.name}
                        disabled={row.state === "cloning" || row.state === "cloned"}
                        aria-invalid={row.check?.key === targetKey(row) && row.check.error !== null}
                        onInput={(event) => {
                          const name = event.currentTarget.value;
                          change(index(), (draft) => (draft.name = name));
                          recheck(index());
                        }}
                      />
                    </div>
                    <Show when={row.state === "idle" && row.check?.key === targetKey(row) && row.check.error}>
                      {(why) => <span class="field-error">{why()}</span>}
                    </Show>
                    <Show when={row.state !== "idle"}>
                      <span class="missing-state" role="status">
                        {row.state === "cloning"
                          ? strings.clone.missing.cloning
                          : row.state === "cloned"
                            ? strings.clone.missing.cloned
                            : strings.clone.missing.failed(row.message ?? strings.clone.missing.cloneFailed)}
                      </span>
                    </Show>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </Show>
        <Refusal error={error()} />
        <div class="dialog-actions">
          <button type="button" onClick={close}>
            {strings.common.close}
          </button>
          <button type="submit" class="primary" disabled={!rows.some(clonable)}>
            {strings.clone.missing.cloneSelected}
          </button>
        </div>
      </form>
    </Frame>
  );
}
