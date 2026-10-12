// The full-window setup screen shown while no usable git is found (PLAT-3,
// D13). Paths, versions and git's output are rendered as text (SEC-3).
import { For, Match, Show, Switch } from "solid-js";
import type { GitResult } from "./stores/git";

type Problem = Exclude<GitResult, { kind: "usable" }>;

/** One line per platform, keyed by `AppInfo.os`. */
const INSTALL_HINTS: Record<string, string> = {
  windows: "Install Git for Windows from git-scm.com, or run: winget install --id Git.Git -e",
  macos: "Install a current git with Homebrew: brew install git",
  linux: "Install a current git with your package manager; on older Ubuntu releases, use the git-core PPA.",
};
const DEFAULT_HINT = "Install a current git from git-scm.com.";

export default function GitSetup(props: {
  problem: Problem;
  os: string | undefined;
  checking: boolean;
  onRecheck: () => void;
}) {
  const hint = () => INSTALL_HINTS[props.os ?? ""] ?? DEFAULT_HINT;

  return (
    <main>
      <h1>gitcroc needs git</h1>
      <Switch>
        <Match when={props.problem.kind === "not_found" && props.problem}>
          {(problem) => (
            <>
              <p role="alert">git was not found.</p>
              <Show when={problem().searched.length > 0} fallback={<p>There was nowhere to look: PATH is empty.</p>}>
                <p>Looked for it at:</p>
                <ul>
                  <For each={problem().searched}>{(path) => <li>{path}</li>}</For>
                </ul>
              </Show>
            </>
          )}
        </Match>
        <Match when={props.problem.kind === "too_old" && props.problem}>
          {(problem) => (
            <p role="alert">
              Found git {problem().found} at {problem().path}, but gitcroc needs git {problem().required} or newer.
            </p>
          )}
        </Match>
        <Match when={props.problem.kind === "unparseable" && props.problem}>
          {(problem) => (
            <>
              <p role="alert">Found git at {problem().path}, but could not read its version. It reported:</p>
              <pre>{problem().output}</pre>
            </>
          )}
        </Match>
      </Switch>
      <p>{hint()}</p>
      <button type="button" disabled={props.checking} onClick={() => props.onRecheck()}>
        {props.checking ? "Checking…" : "Check again"}
      </button>
    </main>
  );
}
