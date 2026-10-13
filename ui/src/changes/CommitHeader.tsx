// A commit's header (UI-14): full hash, author and committer with dates,
// parents and the full message. A parent is a button that selects it in the
// history; a merge commit can be compared with any parent (UI-16). All of it
// is repo text, rendered as text (SEC-3).
import { For, Show } from "solid-js";
import { formatDate, formatFullDate } from "../history/rows";
import type { CommitDetails, Person } from "../ipc";

function PersonLine(props: { role: string; person: Person }) {
  return (
    <>
      <dt>{props.role}</dt>
      <dd>
        <span title={props.person.email}>{props.person.name}</span>{" "}
        <span class="commit-email">&lt;{props.person.email}&gt;</span>{" "}
        <time title={formatFullDate(props.person.time.seconds)}>{formatDate(props.person.time.seconds)}</time>
      </dd>
    </>
  );
}

export default function CommitHeader(props: {
  details: CommitDetails;
  /** The parent the files are compared with. */
  parent: number;
  onSelectParent: (hash: string) => void;
  onCompareWith: (parent: number) => void;
}) {
  return (
    <section class="commit-header" aria-label="Commit">
      <dl>
        <dt>Commit</dt>
        <dd class="hash commit-hash">{props.details.hash}</dd>
        <PersonLine role="Author" person={props.details.author} />
        <PersonLine role="Committer" person={props.details.committer} />
        <dt>{props.details.parents.length === 1 ? "Parent" : "Parents"}</dt>
        <dd class="commit-parents">
          <Show when={props.details.parents.length > 0} fallback="none (the first commit)">
            <For each={props.details.parents}>
              {(hash) => (
                <button type="button" class="link hash" title={`Select ${hash}`} onClick={() => props.onSelectParent(hash)}>
                  {hash.slice(0, 10)}
                </button>
              )}
            </For>
          </Show>
        </dd>
        <Show when={props.details.parents.length > 1}>
          <dt>Compare with</dt>
          <dd>
            <select
              aria-label="Compare with parent"
              value={props.parent}
              onChange={(event) => props.onCompareWith(Number(event.currentTarget.value))}
            >
              <For each={props.details.parents}>
                {(hash, index) => (
                  <option value={index()}>
                    {index() === 0 ? "First parent" : `Parent ${index() + 1}`} ({hash.slice(0, 10)})
                  </option>
                )}
              </For>
            </select>
          </dd>
        </Show>
      </dl>
      <pre class="commit-message">{props.details.message}</pre>
    </section>
  );
}
