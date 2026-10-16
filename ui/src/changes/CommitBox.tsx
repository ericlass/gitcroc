// The commit message box under the file lists of "Uncommitted changes"
// (UI-15): a subject line, a body, Commit and Amend. For a group or several
// repos (UI-34) the Commit button names how many repos it commits, the
// group Commit's options sit above it, and there is no Amend. It shows what
// the commit store holds and forwards input; the core composes the message
// and refuses what cannot be committed (ARCH-6).
import { type JSX, Show } from "solid-js";
import type { CommitMessage } from "../ipc";

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
    <section class="commit-box" aria-label="Commit message">
      <input
        type="text"
        class="commit-subject"
        aria-label="Subject"
        placeholder="Subject"
        spellcheck={true}
        value={props.draft.subject}
        onInput={(event) => props.onEdit({ subject: event.currentTarget.value })}
        onKeyDown={onKeyDown}
      />
      <textarea
        class="commit-body"
        aria-label="Body"
        placeholder="Body (optional)"
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
              title="Replace the last commit with this message and the staged changes. With an empty box, fill in the last commit's message first."
              onClick={() => amend()()}
            >
              Amend
            </button>
          )}
        </Show>
        <button type="button" class="primary" disabled={!canCommit()} onClick={() => props.onCommit()}>
          {props.busy ? "Committing…" : (props.label ?? "Commit")}
        </button>
      </div>
    </section>
  );
}
