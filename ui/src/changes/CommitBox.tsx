// The commit message box under the file lists of "Uncommitted changes"
// (UI-15): a subject line, a body, Commit and Amend. For a group or several
// repos (UI-34) the Commit button names how many repos it commits, the
// group Commit's options sit above it, and there is no Amend. It shows what
// the commit store holds and forwards input; the core composes the message
// and refuses what cannot be committed (ARCH-6).
import { type JSX, Show } from "solid-js";
import type { CommitMessage } from "../ipc";
import { strings } from "../strings/en";

export default function CommitBox(props: {
  draft: CommitMessage;
  /** Something is staged. */
  staged: boolean;
  /** There is a last commit to amend. */
  amendable: boolean;
  /** A commit or amend of this repo is running. */
  busy: boolean;
  error: string | null;
  /** What the Commit button says; "Commit" by default. */
  label?: string;
  onEdit: (change: Partial<CommitMessage>) => void;
  onCommit: () => void;
  /** Absent where Amend is not offered (UI-34). */
  onAmend?: () => void;
  /** Options shown above the buttons. */
  children?: JSX.Element;
}) {
  const canCommit = () => !props.busy && props.staged && props.draft.subject.trim() !== "";

  // Ctrl or Cmd and Enter commits from either field.
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey) && canCommit()) {
      event.preventDefault();
      props.onCommit();
    }
  };

  return (
    <section class="commit-box" aria-label={strings.changes.commitMessage}>
      <input
        type="text"
        class="commit-subject"
        aria-label={strings.changes.subject}
        placeholder={strings.changes.subject}
        spellcheck={true}
        value={props.draft.subject}
        onInput={(event) => props.onEdit({ subject: event.currentTarget.value })}
        onKeyDown={onKeyDown}
      />
      <textarea
        class="commit-body"
        aria-label={strings.changes.body}
        placeholder={strings.changes.bodyPlaceholder}
        rows={3}
        spellcheck={true}
        value={props.draft.body}
        onInput={(event) => props.onEdit({ body: event.currentTarget.value })}
        onKeyDown={onKeyDown}
      />
      {props.children}
      <Show when={props.error}>{(error) => <p role="alert">{error()}</p>}</Show>
      <div class="commit-actions">
        <Show when={props.onAmend}>
          {(amend) => (
            <button
              type="button"
              disabled={props.busy || !props.amendable}
              title={strings.changes.amendTitle}
              onClick={() => amend()()}
            >
              {strings.changes.amend}
            </button>
          )}
        </Show>
        <button type="button" class="primary" disabled={!canCommit()} onClick={() => props.onCommit()}>
          {props.busy ? strings.changes.committing : (props.label ?? strings.changes.commit)}
        </button>
      </div>
    </section>
  );
}
