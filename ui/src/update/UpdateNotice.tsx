// The status bar's notice of a new version (PLAT-4): shown while one is on offer and not put off with Later.
// Clicking it opens the update dialog. The window's live region says it once when it appears (M5-10).
import { Show } from "solid-js";
import { noticeOf, type UpdateStore } from "../stores/update";
import { strings } from "../strings/en";

export default function UpdateNotice(props: { update: UpdateStore; onOpen: () => void }) {
  return (
    <Show when={noticeOf(props.update.state.view)}>
      {(version) => (
        <button type="button" class="status-update" aria-haspopup="dialog" onClick={() => props.onOpen()}>
          {strings.update.notice(version())}
        </button>
      )}
    </Show>
  );
}
