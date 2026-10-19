// The editor of a text conflict (OPS-3, OPS-5, OPS-9): the buffer in an
// editable CodeMirror with its hunks, next and previous conflict, and Mark
// resolved, which sends the whole buffer to the core. The editor state is
// kept in the conflict store on every change and restored with its cursor,
// undo and scroll when the file is shown again. Nothing is written here.
// With AI features on (AI-16), the header has Resolve with AI for the hunks
// still untouched (AI-3), and a proposal shows under its hunks (AI-11) with
// the token usage of each request (AI-14). Accepting one edits the buffer
// only (AI-12). The palette's conflict operations run its toolbar's buttons
// and the hunk keys, by their rules (UI-22).
import { EditorState, StateEffect, type Text } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { createEffect, createSignal, For, on, onCleanup, onMount, Show, untrack } from "solid-js";
import { applyLanguage, baseExtensions, newParts, wrapping } from "../changes/codemirror";
import type { FileProposal, WorkspaceId } from "../ipc";
import { type Command, type CommandStore, commandOf, type Providers } from "../palette/commands";
import type { AiStore } from "../stores/ai";
import type { AiAsk, ConflictStore, FileConflict, Start, ViewFile } from "../stores/conflicts";
import { buildBuffer, normalize } from "./buffer";
import {
  acceptAllHigh,
  conflictExtensions,
  goToHunk,
  highConfidence,
  hunkCount,
  inHunk,
  showProposal,
  takeAtCursor,
  untouchedHunks,
} from "./editor";
import type { Take } from "./buffer";
import { strings } from "../strings/en";

/** What the editor needs for AI; without it the editor has none. */
export interface ConflictAi {
  store: AiStore;
  /** AI for this workspace (AI-6). */
  workspaceOn: () => boolean;
  /** Opens Settings at its AI section (AI-3). */
  openSettings: () => void;
}

/** What resolving with AI asks of the app: consent, the provider settings and the provider's name (AI-3, AI-6). */
export function aiAsk(ai: ConflictAi): AiAsk {
  return {
    askConsent: (provider) => ai.store.askConsent(provider),
    openSettings: () => ai.openSettings(),
    providerName: () => {
      const settings = ai.store.state.view;
      const provider = settings?.providers.find((provider) => provider.id === settings.default);
      return provider?.name ?? strings.conflict.theProvider;
    },
  };
}

const count = (value: number) => value.toLocaleString("en");

/** The token usage of each request the provider reported it for (AI-14). */
const usageLines = (proposal: FileProposal) =>
  proposal.usage.flatMap((usage, index) => {
    if (usage === null) {
      return [];
    }
    const tokens = strings.conflict.tokens(count(usage.input_tokens), count(usage.output_tokens));
    return [strings.conflict.request(index + 1, tokens)];
  });

/** The buffer a file starts with: the conflict's hunks, or the file on disk without any. */
function startBuffer(file: FileConflict, start: Start) {
  const content = file.conflict.content;
  if (start === "disk" || content.kind !== "text") {
    return { text: normalize(file.conflict.on_disk ?? ""), hunks: [] };
  }
  return buildBuffer(content.chunks, file.conflict.labels);
}

export default function ConflictEditor(props: {
  workspace: WorkspaceId;
  target: ViewFile;
  file: FileConflict;
  start: Start;
  conflicts: ConflictStore;
  wrap: boolean;
  /** Takes the focus when shown, as when the file before it was resolved from the keyboard. */
  focus: boolean;
  /** Called on closing, with whether the editor had the focus. */
  onClose: (focused: boolean) => void;
  ai?: ConflictAi;
  commands: CommandStore;
}) {
  let parent: HTMLDivElement | undefined;
  let view: EditorView | undefined;
  const parts = newParts();
  const [remaining, setRemaining] = createSignal(0);
  /** The hunks Resolve with AI sends, and how many Accept all high-confidence takes. */
  const [untouched, setUntouched] = createSignal<number[]>([]);
  const [high, setHigh] = createSignal(0);
  const counted = (state: EditorState) => {
    setRemaining(hunkCount(state));
    setUntouched(untouchedHunks(state));
    setHigh(highConfidence(state).length);
  };
  // The file this editor was opened for; the parent remounts it for another.
  const { workspace, conflicts, repo, path } = untrack(() => ({
    workspace: props.workspace,
    conflicts: props.conflicts,
    repo: props.target.repo,
    path: props.target.path,
  }));

  const markResolved = (override: boolean) => {
    if (view !== undefined && !props.file.sending) {
      void conflicts.resolve(workspace, props.target, view.state.doc.toString(), override);
    }
  };

  onMount(() => {
    const saved = conflicts.editor(workspace, repo, path);
    const actions = {
      labels: props.file.conflict.labels,
      markResolved: () => markResolved(false),
      reject: (hunk: number) => conflicts.rejectHunk(workspace, repo, path, hunk),
      edit: (hunk: number, text: string | null) => conflicts.editHunk(workspace, repo, path, hunk, text),
    };
    const wrap = props.wrap;
    // What the buffer started with, to tell unsaved edits (OPS-9).
    let initial: Text;
    const extensions = (hunks: Parameters<typeof conflictExtensions>[1]) => [
      ...baseExtensions(parts, wrap, true),
      ...conflictExtensions(actions, hunks),
      EditorView.updateListener.of((update) => {
        if (update.docChanged || update.transactions.length > 0) {
          counted(update.state);
          conflicts.keep(workspace, repo, path, { state: update.state, initial, scroll: null });
        }
      }),
      EditorView.contentAttributes.of({ "aria-label": strings.conflict.resolution(path) }),
    ];
    let state: EditorState;
    if (saved !== undefined) {
      initial = saved.initial;
      state = saved.state.update({ effects: StateEffect.reconfigure.of(extensions(null)) }).state;
    } else {
      const buffer = startBuffer(props.file, props.start);
      state = EditorState.create({ doc: buffer.text, extensions: extensions(buffer.hunks) });
      initial = state.doc;
      conflicts.keep(workspace, repo, path, { state, initial, scroll: null });
    }
    view = new EditorView({ state, parent });
    counted(state);
    if (saved?.scroll) {
      view.dispatch({ effects: saved.scroll });
    }
    void applyLanguage(parts, path, [view]);
    if (props.focus) {
      view.focus();
    }
    const opened = view;
    onCleanup(() => {
      props.onClose(opened.hasFocus);
      conflicts.keep(workspace, repo, path, {
        state: opened.state,
        initial,
        scroll: opened.scrollSnapshot(),
      });
      opened.destroy();
    });
  });

  createEffect(
    on(
      () => props.wrap,
      (wrap) => view?.dispatch({ effects: wrapping(parts, wrap) }),
      { defer: true },
    ),
  );

  const aiOn = () => props.ai?.store.enabled() === true;

  // The proposal as the store keeps it, in the editor; none while AI features are off (AI-16).
  createEffect(() => {
    const proposal = aiOn() ? props.file.ai.proposal : null;
    const shown =
      proposal === null
        ? null
        : {
            hunks: proposal.hunks.map((hunk) => ({ ...hunk })),
            views: Object.fromEntries(
              Object.entries(props.file.ai.hunks).map(([id, hunk]) => [id, hunk && { ...hunk }]),
            ),
          };
    if (view !== undefined) {
      showProposal(view, shown);
    }
  });

  // Whether the file may go to AI (AI-4), read again when the exclude list changes.
  createEffect(
    on(
      () => [aiOn(), props.ai?.store.state.view?.exclude.join("\n")] as const,
      ([on]) => on && void conflicts.readEligibility(workspace, untrack(() => props.target)),
    ),
  );

  // A result the core kept is read while the file is shown, through its newest view.
  createEffect(
    on(
      () => [aiOn() && props.file.ai.ready, props.target.handle] as const,
      ([ready]) => ready && void conflicts.readProposal(workspace, untrack(() => props.target)),
    ),
  );

  /** Why Resolve with AI is disabled, shown beside it (AI-4, AI-6). */
  const aiBlocked = () => {
    const eligibility = props.file.ai.eligibility;
    if (props.ai?.workspaceOn() === false) {
      return strings.conflict.aiOffHere;
    }
    return eligibility?.kind === "ineligible" ? strings.conflict.notSentToAi(eligibility.message) : null;
  };

  const resolveWithAi = () => {
    const ai = props.ai;
    if (ai === undefined || view === undefined) {
      return;
    }
    void conflicts.resolveWithAi(workspace, props.target, untouchedHunks(view.state), aiAsk(ai));
  };
  const canResolve = () => aiBlocked() === null && !props.file.ai.starting && untouched().length > 0;

  /** The palette's Take ours, theirs or both: what the hunk keys do, on the hunk the cursor is in. */
  const takeSide = (side: Take) => (): Command =>
    view !== undefined && inHunk(view.state)
      ? { run: () => view !== undefined && takeAtCursor(side)(view) }
      : { reason: strings.palette.noHunk };
  const paletteCommands: Providers<"editor"> = {
    mark_resolved: () => (props.file.sending ? commandOf(null) : { run: () => markResolved(false) }),
    take_ours: takeSide("ours"),
    take_theirs: takeSide("theirs"),
    take_both: takeSide("both"),
    // As the toolbar's button, shown while AI features are on and no request of the file runs.
    resolve_with_ai: () =>
      !aiOn()
        ? null
        : commandOf(props.file.ai.job === null && canResolve() ? resolveWithAi : null, aiBlocked() ?? undefined),
    accept_high_confidence: () =>
      aiOn() && props.file.ai.proposal !== null && high() > 0
        ? { run: () => view !== undefined && acceptAllHigh(view) }
        : commandOf(null),
  };
  createEffect(() => onCleanup(props.commands.provide(workspace, "editor", paletteCommands)));

  const go = (direction: 1 | -1) => {
    if (view !== undefined) {
      goToHunk(view, direction);
      view.focus();
    }
  };

  return (
    <div class="conflict-editor-pane">
      <div class="conflict-toolbar" role="toolbar" aria-label={strings.conflict.toolbar}>
        <span class="conflict-count" role="status">
          {strings.conflict.left(remaining())}
        </span>
        <button
          type="button"
          title={strings.conflict.keys.previous}
          disabled={remaining() === 0}
          onClick={() => go(-1)}
        >
          {strings.conflict.previous}
        </button>
        <button type="button" title={strings.conflict.keys.next} disabled={remaining() === 0} onClick={() => go(1)}>
          {strings.conflict.next}
        </button>
        <button
          type="button"
          class="primary"
          title={strings.conflict.markResolvedTitle}
          disabled={props.file.sending}
          onClick={() => markResolved(false)}
        >
          {strings.conflict.markResolved}
        </button>
        <Show when={aiOn()}>
          <span class="conflict-ai">
            <Show
              when={props.file.ai.job}
              fallback={
                <button
                  type="button"
                  title={strings.conflict.resolveWithAiTitle}
                  disabled={!canResolve()}
                  onClick={resolveWithAi}
                >
                  {strings.conflict.resolveWithAi}
                </button>
              }
            >
              {(job) => (
                <button type="button" onClick={() => conflicts.cancelAi(workspace, repo, path)}>
                  {strings.conflict.cancelAi(job().provider)}
                </button>
              )}
            </Show>
            <Show when={aiBlocked() ?? props.file.ai.refusal}>
              {(reason) => <span class="conflict-ai-reason">{reason()}</span>}
            </Show>
            <Show when={props.file.ai.failed}>
              {(reason) => (
                <>
                  <span role="alert" class="conflict-ai-failed">
                    {strings.conflict.aiFailed(reason())}
                  </span>
                  <Show when={props.file.ai.job === null}>
                    <button type="button" disabled={!canResolve()} onClick={resolveWithAi}>
                      {strings.conflict.tryAgain}
                    </button>
                  </Show>
                </>
              )}
            </Show>
            <Show when={props.file.ai.proposal}>
              {(proposal) => (
                <>
                  <button
                    type="button"
                    title={strings.conflict.acceptAllHighTitle}
                    disabled={high() === 0}
                    onClick={() => view !== undefined && acceptAllHigh(view)}
                  >
                    {strings.conflict.acceptAllHigh}
                  </button>
                  <Show when={usageLines(proposal()).length > 0}>
                    <ul class="conflict-ai-usage" aria-label={strings.conflict.tokenUsage}>
                      <For each={usageLines(proposal())}>{(line) => <li>{line}</li>}</For>
                    </ul>
                  </Show>
                </>
              )}
            </Show>
          </span>
        </Show>
      </div>
      <Show when={props.file.markers}>
        {(line) => (
          <p role="alert" class="conflict-refusal">
            {strings.conflict.markerLine(line())}
            <button type="button" disabled={props.file.sending} onClick={() => markResolved(true)}>
              {strings.conflict.markResolvedAnyway}
            </button>
          </p>
        )}
      </Show>
      <Show when={props.file.error}>
        {(error) => <p role="alert">{strings.conflict.cannotMarkResolved(error())}</p>}
      </Show>
      <div class="conflict-editor" ref={(element) => (parent = element)} />
    </div>
  );
}
