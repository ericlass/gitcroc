// What the changes pane shows for an unmerged file (OPS-3, OPS-6, OPS-8):
// the conflict editor for text, the question first when the file was
// edited outside the app, or a choice between the two versions. The core
// reads the conflict and says which it is; labels and paths are repo text,
// rendered as text (SEC-3).
import { createEffect, Match, on, Show, Switch } from "solid-js";
import type { ChoiceKind, ConflictChoice, SideHolds, WorkspaceId } from "../ipc";
import type { ConflictStore, ViewFile } from "../stores/conflicts";
import ConflictEditor, { type ConflictAi } from "./ConflictEditor";

const REASONS: Record<ChoiceKind, string> = {
  binary: "The file is binary.",
  deleted_by_us: "Our side deleted the file and theirs changed it.",
  deleted_by_them: "Their side deleted the file and ours changed it.",
  both_deleted: "Neither side kept the file at this path.",
  mode: "Both sides changed the file's mode differently.",
  submodule: "A side is a submodule.",
  type_change: "One side has a file and the other a symbolic link.",
  symlink: "Both sides changed the symbolic link.",
  not_utf8: "The file is not UTF-8 text.",
  too_large: "The file is too large to edit here.",
  marker_like: "The file has lines that cannot be told apart from conflict markers.",
};

const bytes = (size: number) => (size === 1 ? "1 byte" : `${size.toLocaleString("en")} bytes`);

/** What a side holds, in words. */
export function holdsText(holds: SideHolds): string {
  switch (holds.kind) {
    case "deleted":
      return "Deleted";
    case "file":
      return `File, ${bytes(holds.size)}${holds.executable ? ", executable" : ""}`;
    case "symlink":
      return `Symbolic link, ${bytes(holds.size)}`;
    case "submodule":
      return `Submodule at commit ${holds.commit.slice(0, 12)}`;
  }
}

export default function ConflictView(props: {
  workspace: WorkspaceId;
  target: ViewFile;
  conflicts: ConflictStore;
  wrap: boolean;
  ai?: ConflictAi;
}) {
  const file = () => props.conflicts.file(props.workspace, props.target.repo, props.target.path);
  /** The last editor closed with the focus, so the next one takes it: resolving file after file from the keyboard. */
  let refocus = false;

  // Reads the file unless its state is kept (OPS-9).
  createEffect(
    on(
      () => [props.workspace, props.target.repo, props.target.path] as const,
      () => void props.conflicts.open(props.workspace, props.target),
    ),
  );

  const choice = () => {
    const content = file()?.conflict.content;
    return content?.kind === "choice" ? content : null;
  };
  /** The editor's key: a new file or a new start remounts it. */
  const editing = () => {
    const current = file();
    return current === undefined || current.start === null || choice() !== null
      ? null
      : `${props.target.repo}\n${props.target.path}\n${current.start}`;
  };
  const choose = (side: ConflictChoice) => void props.conflicts.choose(props.workspace, props.target, side);

  return (
    <div class="conflict-view">
      <Switch
        fallback={
          <Show
            when={props.conflicts.error(props.workspace)}
            fallback={<p class="placeholder">Reading the conflict…</p>}
          >
            {(error) => <p role="alert">Cannot read the conflict: {error()}</p>}
          </Show>
        }
      >
        <Match when={file()?.start === null && file()}>
          {(current) => (
            <div class="conflict-ask" role="group" aria-label="Edited outside">
              <p>
                {props.target.path} was changed outside gitcroc since the conflict began. Keep the file as it is, or
                start over from the conflict?
              </p>
              <Show when={current().conflict.on_disk === null}>
                <p>The file cannot be shown as it is: it is missing, not UTF-8 text, or too large.</p>
              </Show>
              <button
                type="button"
                disabled={current().conflict.on_disk === null}
                onClick={() => props.conflicts.answer(props.workspace, props.target.repo, props.target.path, "disk")}
              >
                Keep the file as it is
              </button>
              <button
                type="button"
                onClick={() =>
                  props.conflicts.answer(props.workspace, props.target.repo, props.target.path, "conflict")
                }
              >
                Start over from the conflict
              </button>
            </div>
          )}
        </Match>
        <Match when={choice()}>
          {(content) => (
            <div class="conflict-choice">
              <p>{REASONS[content().reason]} Choose the version to keep.</p>
              <div class="conflict-sides">
                <section class="conflict-side" aria-label={`Ours: ${file()?.conflict.labels.ours ?? ""}`}>
                  <h3>Ours: {file()?.conflict.labels.ours}</h3>
                  <p>{holdsText(content().ours)}</p>
                  <button type="button" disabled={file()?.sending} onClick={() => choose("ours")}>
                    Use this version
                  </button>
                </section>
                <section class="conflict-side" aria-label={`Theirs: ${file()?.conflict.labels.theirs ?? ""}`}>
                  <h3>Theirs: {file()?.conflict.labels.theirs}</h3>
                  <p>{holdsText(content().theirs)}</p>
                  <button type="button" disabled={file()?.sending} onClick={() => choose("theirs")}>
                    Use this version
                  </button>
                </section>
              </div>
              <Show when={file()?.error}>{(error) => <p role="alert">Cannot resolve it: {error()}</p>}</Show>
            </div>
          )}
        </Match>
        <Match when={editing()}>
          <Show when={editing()} keyed>
            {(key) => (
              <Show when={file()}>
                {(current) => (
                  <ConflictEditor
                    workspace={props.workspace}
                    target={props.target}
                    file={current()}
                    start={key.endsWith("\ndisk") ? "disk" : "conflict"}
                    conflicts={props.conflicts}
                    wrap={props.wrap}
                    focus={refocus}
                    onClose={(focused) => (refocus = focused)}
                    ai={props.ai}
                  />
                )}
              </Show>
            )}
          </Show>
        </Match>
      </Switch>
    </div>
  );
}
