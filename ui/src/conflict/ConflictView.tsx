// What the changes pane shows for an unmerged file (OPS-3, OPS-6, OPS-8):
// the conflict editor for text, the question first when the file was
// edited outside the app, or a choice between the two versions. The core
// reads the conflict and says which it is; labels and paths are repo text,
// rendered as text (SEC-3). The palette's Keep ours and Keep theirs run the
// choice's buttons (UI-22).
import { createEffect, Match, on, onCleanup, Show, Switch } from "solid-js";
import type { ChoiceKind, ConflictChoice, SideHolds, WorkspaceId } from "../ipc";
import { type Command, type CommandStore, commandOf, type Providers } from "../palette/commands";
import type { ConflictStore, ViewFile } from "../stores/conflicts";
import ConflictEditor, { type ConflictAi, type EditorFocus } from "./ConflictEditor";
import { strings } from "../strings/en";

const REASONS: Record<ChoiceKind, string> = strings.conflict.reasons;

/** What a side holds, in words. */
export function holdsText(holds: SideHolds): string {
  switch (holds.kind) {
    case "deleted":
      return strings.conflict.deleted;
    case "file":
      return strings.conflict.file(strings.conflict.bytes(holds.size), holds.executable);
    case "symlink":
      return strings.conflict.symlink(strings.conflict.bytes(holds.size));
    case "submodule":
      return strings.conflict.submodule(holds.commit.slice(0, 12));
  }
}

export default function ConflictView(props: {
  workspace: WorkspaceId;
  target: ViewFile;
  conflicts: ConflictStore;
  wrap: boolean;
  ai?: ConflictAi;
  commands: CommandStore;
}) {
  const file = () => props.conflicts.file(props.workspace, props.target.repo, props.target.path);
  /** The last editor closed with the focus, so the next one takes it: resolving file after file from the keyboard. */
  let refocus: EditorFocus = null;

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

  /** The palette's Keep ours or Keep theirs: the choice's button of `side`, for a file that has the choice. */
  const keep = (side: ConflictChoice) => (): Command | null =>
    file() === undefined
      ? null
      : choice() === null
        ? { reason: strings.palette.notChoice }
        : file()?.sending === true
          ? commandOf(null)
          : { run: () => choose(side) };
  const paletteCommands: Providers<"choice"> = { keep_ours: keep("ours"), keep_theirs: keep("theirs") };
  createEffect(() => onCleanup(props.commands.provide(props.workspace, "choice", paletteCommands)));

  return (
    <div class="conflict-view">
      <Switch
        fallback={
          <Show
            when={props.conflicts.error(props.workspace)}
            fallback={<p class="placeholder">{strings.conflict.reading}</p>}
          >
            {(error) => <p role="alert">{strings.conflict.cannotRead(error())}</p>}
          </Show>
        }
      >
        <Match when={file()?.start === null && file()}>
          {(current) => (
            <div class="conflict-ask" role="group" aria-label={strings.conflict.editedOutside}>
              <p>{strings.conflict.changedOutside(props.target.path)}</p>
              <Show when={current().conflict.on_disk === null}>
                <p>{strings.conflict.cannotShowDisk}</p>
              </Show>
              <button
                type="button"
                disabled={current().conflict.on_disk === null}
                onClick={() => props.conflicts.answer(props.workspace, props.target.repo, props.target.path, "disk")}
              >
                {strings.conflict.keepDisk}
              </button>
              <button
                type="button"
                onClick={() =>
                  props.conflicts.answer(props.workspace, props.target.repo, props.target.path, "conflict")
                }
              >
                {strings.conflict.startOver}
              </button>
            </div>
          )}
        </Match>
        <Match when={choice()}>
          {(content) => (
            <div class="conflict-choice">
              <p>{strings.conflict.choose(REASONS[content().reason])}</p>
              <div class="conflict-sides">
                <section class="conflict-side" aria-label={strings.conflict.ours(file()?.conflict.labels.ours ?? "")}>
                  <h3>{strings.conflict.ours(file()?.conflict.labels.ours ?? "")}</h3>
                  <p>{holdsText(content().ours)}</p>
                  <button type="button" disabled={file()?.sending} onClick={() => choose("ours")}>
                    {strings.conflict.useThis}
                  </button>
                </section>
                <section
                  class="conflict-side"
                  aria-label={strings.conflict.theirs(file()?.conflict.labels.theirs ?? "")}
                >
                  <h3>{strings.conflict.theirs(file()?.conflict.labels.theirs ?? "")}</h3>
                  <p>{holdsText(content().theirs)}</p>
                  <button type="button" disabled={file()?.sending} onClick={() => choose("theirs")}>
                    {strings.conflict.useThis}
                  </button>
                </section>
              </div>
              <Show when={file()?.error}>
                {(error) => <p role="alert">{strings.conflict.cannotResolve(error())}</p>}
              </Show>
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
                    commands={props.commands}
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
