// The update dialog (PLAT-4): the running and the new version, the release notes as plain text (SEC-3), and
// what the package does with an update: Install and restart for the Windows installer, the macOS app and the
// AppImage, Open download page for the `.deb`, the `.rpm` and the portable zip. Later hides the notice until the
// next start or a newer version. Installing restarts the app, so with unsaved conflict edits in a tab it asks
// first, as closing that tab does (OPS-9).
import { createSignal, For, Show } from "solid-js";
import { Modal, Question, Refusal } from "../dialogs/parts";
import type { UpdateStore } from "../stores/update";
import { strings } from "../strings/en";

/** A manifest's RFC 3339 date as the local date; one that does not parse is shown as it is. */
export function formatDate(date: string): string {
  const parsed = new Date(date);
  return Number.isNaN(parsed.getTime()) ? date : parsed.toLocaleDateString();
}

export default function UpdateDialog(props: {
  update: UpdateStore;
  /** The files with unsaved conflict edits in any tab, as `workspace: repo: path`. */
  unsaved: () => string[];
  onClose: () => void;
}) {
  const [asking, setAsking] = createSignal(false);
  const [sending, setSending] = createSignal(false);
  const view = () => props.update.state.view;
  const offer = () => {
    const status = view()?.status;
    return status?.kind === "available" ? status : null;
  };

  const install = async () => {
    setSending(true);
    const started = await props.update.install();
    setSending(false);
    setAsking(false);
    if (started) {
      props.onClose();
    }
  };

  const onInstall = () => {
    if (props.unsaved().length > 0) {
      setAsking(true);
    } else {
      void install();
    }
  };

  const onDownload = async () => {
    await props.update.openDownloadPage();
    if (props.update.state.error === null) {
      props.onClose();
    }
  };

  const onLater = () => {
    void props.update.later();
    props.onClose();
  };

  return (
    <>
      <Modal labelledby="update-title" class="dialog update-dialog" onEscape={props.onClose}>
        <h2 id="update-title">{strings.update.title}</h2>
        <Show when={offer()}>
          {(offered) => (
            <>
              <p>{strings.update.versions(view()?.current_version ?? "", offered().version)}</p>
              <Show when={offered().date}>{(date) => <p>{strings.update.published(formatDate(date()))}</p>}</Show>
              <h3 id="update-notes-title">{strings.update.notes}</h3>
              <Show when={offered().notes !== ""} fallback={<p>{strings.update.noNotes}</p>}>
                <pre class="update-notes" tabindex="0" aria-labelledby="update-notes-title">
                  {offered().notes}
                </pre>
              </Show>
              <Show when={!view()?.installs}>
                <p>{strings.update.notifyOnly}</p>
              </Show>
            </>
          )}
        </Show>
        <Refusal error={props.update.state.error} />
        <div class="dialog-actions">
          <button type="button" onClick={onLater}>
            {strings.update.later}
          </button>
          <Show
            when={view()?.installs}
            fallback={
              <button type="button" class="primary" disabled={offer() === null} onClick={() => void onDownload()}>
                {strings.update.openDownloadPage}
              </button>
            }
          >
            <button type="button" class="primary" disabled={offer() === null || sending()} onClick={onInstall}>
              {strings.update.install}
            </button>
          </Show>
        </div>
      </Modal>
      <Show when={asking()}>
        <Question
          title={strings.update.unsavedTitle}
          submit={strings.update.updateAnyway}
          error={props.update.state.error}
          sending={sending()}
          onConfirm={() => void install()}
          onClose={() => setAsking(false)}
        >
          <p>{strings.update.unsavedLost}</p>
          <ul class="op-actions">
            <For each={props.unsaved()}>{(file) => <li>{file}</li>}</For>
          </ul>
        </Question>
      </Show>
    </>
  );
}
