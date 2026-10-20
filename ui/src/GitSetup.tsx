// The full-window setup screen shown while no usable git is found (PLAT-3,
// D13). Paths, versions and git's output are rendered as text (SEC-3). A git
// that is not on PATH can be chosen here, and a chosen one given up again.
import { For, Match, Show, Switch } from "solid-js";
import type { GitResult } from "./stores/git";
import { strings } from "./strings/en";

type Problem = Exclude<GitResult, { kind: "usable" }>;

export default function GitSetup(props: {
  problem: Problem;
  os: string | undefined;
  checking: boolean;
  /** Why choosing a git failed. */
  error: string | null;
  onRecheck: () => void;
  onChoose: () => void;
  onAutomatic: () => void;
}) {
  const hint = () => strings.gitSetup.installHints[props.os ?? ""] ?? strings.gitSetup.defaultHint;

  return (
    <main>
      <h1>{strings.gitSetup.title}</h1>
      <Switch>
        <Match when={props.problem.kind === "not_found" && props.problem}>
          {(problem) => (
            <>
              <p role="alert">{strings.gitSetup.notFound}</p>
              <Show when={problem().searched.length > 0} fallback={<p>{strings.gitSetup.nowhereToLook}</p>}>
                <p>{strings.gitSetup.lookedAt}</p>
                <ul>
                  <For each={problem().searched}>{(path) => <li>{path}</li>}</For>
                </ul>
              </Show>
            </>
          )}
        </Match>
        <Match when={props.problem.kind === "too_old" && props.problem}>
          {(problem) => (
            <p role="alert">{strings.gitSetup.tooOld(problem().found, problem().path, problem().required)}</p>
          )}
        </Match>
        <Match when={props.problem.kind === "unparseable" && props.problem}>
          {(problem) => (
            <>
              <p role="alert">{strings.gitSetup.unparseable(problem().path)}</p>
              <pre>{problem().output}</pre>
            </>
          )}
        </Match>
      </Switch>
      <Show when={props.problem.chosen_missing && props.problem.chosen}>
        {(chosen) => <p>{strings.gitSetup.chosenMissing(chosen())}</p>}
      </Show>
      <p>{hint()}</p>
      <Show when={props.error}>{(error) => <p role="alert">{error()}</p>}</Show>
      <div class="setup-actions">
        <button type="button" disabled={props.checking} onClick={() => props.onRecheck()}>
          {props.checking ? strings.gitSetup.checking : strings.gitSetup.checkAgain}
        </button>
        <button type="button" disabled={props.checking} onClick={() => props.onChoose()}>
          {strings.gitSetup.chooseGit}
        </button>
        <Show when={props.problem.chosen !== null}>
          <button type="button" disabled={props.checking} onClick={() => props.onAutomatic()}>
            {strings.gitSetup.useAutomatic}
          </button>
        </Show>
      </div>
    </main>
  );
}
