// The confirmation before a discard (GRP-9): it names the repo and the
// files the core said it would discard, as text (SEC-3). The core refuses
// a discard sent without it; this only asks. Escape and Cancel keep the
// changes.
import { For, onMount } from "solid-js";
import { Modal } from "../dialogs/parts";
import { strings } from "../strings/en";

/** Files named one by one; more are counted. */
const LISTED = 10;

export default function DiscardDialog(props: {
  repo: string;
  files: string[];
  /** One hunk of the file, not the whole file. */
  hunk: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  let cancel: HTMLButtonElement | undefined;

  // Cancel has the focus: Enter alone never discards.
  onMount(() => cancel?.focus());

  const more = () => props.files.length - LISTED;

  return (
    <Modal
      role="alertdialog"
      labelledby="discard-title"
      describedby="discard-text"
      class="dialog discard"
      onEscape={props.onCancel}
    >
      <h2 id="discard-title">{strings.changes.discardDialog(props.repo)}</h2>
      <p id="discard-text">
        {props.hunk
          ? strings.changes.hunkLost
          : props.files.length === 1
            ? strings.changes.fileLost
            : strings.changes.filesLost(props.files.length)}
      </p>
      <ul class="discard-files">
        <For each={props.files.slice(0, LISTED)}>{(file) => <li>{file}</li>}</For>
        {more() > 0 && <li>{strings.changes.andMore(more())}</li>}
      </ul>
      <div class="dialog-actions">
        <button type="button" ref={(element) => (cancel = element)} onClick={() => props.onCancel()}>
          {strings.common.cancel}
        </button>
        <button type="button" class="danger" onClick={() => props.onConfirm()}>
          {strings.changes.discard}
        </button>
      </div>
    </Modal>
  );
}
