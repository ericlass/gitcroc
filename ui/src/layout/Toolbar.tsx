// The toolbar of UI-20. Buttons that cannot apply are disabled, not hidden,
// and say why when it is not plain, as for a group or several repos (GRP-5).
// Fetch, Pull and Push act on the target set: a click runs with the
// defaults, the arrow beside each opens its options. On a group or several
// repos they run through the preflight (section 5). Merge opens the dialog
// that merges or rebases, Stash the stash dialog.

/** What a button does; `null` disables it. */
type Action = (() => void) | null;

export interface SyncActions {
  fetch: Action;
  fetchOptions: Action;
  pull: Action;
  pullOptions: Action;
  push: Action;
  pushOptions: Action;
}

/** Why a button is disabled, by button; shown as its tooltip while it is. */
export type ToolbarReasons = Partial<Record<"fetch" | "pull" | "push" | "branch" | "merge" | "stash", string>>;

/** `title`, or `why` while `action` is disabled. */
const titleOf = (action: Action, title: string, why: string | undefined) => (action === null ? (why ?? title) : title);

/** A button with an arrow beside it that opens its options. */
function SplitButton(props: { name: string; title: string; why: string | undefined; run: Action; options: Action }) {
  return (
    <span class="split-button">
      <button
        type="button"
        disabled={props.run === null}
        title={titleOf(props.run, props.title, props.why)}
        onClick={() => props.run?.()}
      >
        {props.name}
      </button>
      <button
        type="button"
        class="split-arrow"
        disabled={props.options === null}
        aria-label={`${props.name} options`}
        title={titleOf(props.options, `${props.name} with options`, props.why)}
        onClick={() => props.options?.()}
      >
        <span aria-hidden="true">▾</span>
      </button>
    </span>
  );
}

export default function Toolbar(props: {
  sync: SyncActions;
  /** Opens the create branch dialog for the selected repo; `null` when no repo can take it. */
  onBranch: Action;
  /** Opens the merge dialog for the selected repo; `null` when no repo can take it. */
  onMerge: Action;
  /** Opens the stash dialog for the selected repo; `null` when no repo can take it. */
  onStash: Action;
  /** Why buttons are disabled, when it is not plain. */
  disabledWhy?: ToolbarReasons;
  onSettings: () => void;
}) {
  const why = () => props.disabledWhy ?? {};
  return (
    <div role="toolbar" aria-label="Main" class="toolbar">
      <SplitButton
        name="Fetch"
        title="Fetch all remotes"
        why={why().fetch}
        run={props.sync.fetch}
        options={props.sync.fetchOptions}
      />
      <SplitButton
        name="Pull"
        title="Update the current branch from its upstream"
        why={why().pull}
        run={props.sync.pull}
        options={props.sync.pullOptions}
      />
      <SplitButton
        name="Push"
        title="Push the current branch to its upstream"
        why={why().push}
        run={props.sync.push}
        options={props.sync.pushOptions}
      />
      <button
        type="button"
        disabled={props.onBranch === null}
        title={titleOf(props.onBranch, "Create a branch", why().branch)}
        onClick={() => props.onBranch?.()}
      >
        Branch
      </button>
      <button
        type="button"
        disabled={props.onMerge === null}
        title={titleOf(
          props.onMerge,
          "Merge a branch into the current branch, or rebase onto it",
          why().merge,
        )}
        onClick={() => props.onMerge?.()}
      >
        Merge
      </button>
      <button
        type="button"
        disabled={props.onStash === null}
        title={titleOf(props.onStash, "Stash the local changes of the selected repository", why().stash)}
        onClick={() => props.onStash?.()}
      >
        Stash
      </button>
      <span class="toolbar-gap" />
      <button type="button" disabled>
        Command palette
      </button>
      <button type="button" onClick={() => props.onSettings()}>
        Settings
      </button>
    </div>
  );
}
