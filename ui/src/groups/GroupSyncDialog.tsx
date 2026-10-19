// The options of Fetch, Pull and Push on a target set (section 5 "Fetch",
// "Pull" and "Push" rows, UI-20), opened by the arrow beside each toolbar
// button. Running goes through the preflight like a click does (GRP-9): this
// dialog only picks the options. The target's name is shown as text (SEC-3).
import { createSignal, For, Match, Switch } from "solid-js";
import { Actions, Frame } from "../dialogs/parts";
import type { GroupOp, Prune, PullStrategy } from "../ipc";
import type { SyncKind } from "../stores/groupOps";
import { STRATEGIES } from "../sync/SyncDialogs";
import { strings } from "../strings/en";

const PRUNES: { prune: Prune; label: string }[] = [
  { prune: "config", label: strings.groupOps.prunes.config },
  { prune: "prune", label: strings.groupOps.prunes.prune },
  { prune: "no_prune", label: strings.groupOps.prunes.noPrune },
];

const NAMES: Record<SyncKind, string> = strings.groupOps.syncNames;

export default function GroupSyncDialog(props: {
  kind: SyncKind;
  /** What it runs on, such as `Backend` or `3 repositories`. */
  targetName: string;
  onRun: (op: GroupOp) => void;
  onClose: () => void;
}) {
  const [prune, setPrune] = createSignal<Prune>("config");
  const [strategy, setStrategy] = createSignal<PullStrategy>("config");
  const [setUpstream, setSetUpstream] = createSignal(true);
  const [force, setForce] = createSignal(false);
  const op = (): GroupOp => {
    switch (props.kind) {
      case "fetch":
        return { kind: "fetch", prune: prune() };
      case "pull":
        return { kind: "pull", strategy: strategy() };
      case "push":
        return { kind: "push", set_upstream: setUpstream(), force_with_lease: force() };
    }
  };
  return (
    <Frame
      id="group-sync"
      title={strings.groupOps.syncTitle(NAMES[props.kind], props.targetName)}
      onClose={() => props.onClose()}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          props.onRun(op());
        }}
      >
        <Switch>
          <Match when={props.kind === "fetch"}>
            <fieldset>
              <legend>{strings.groupOps.prune}</legend>
              <For each={PRUNES}>
                {(choice) => (
                  <label class="choice">
                    <input
                      type="radio"
                      name="prune"
                      checked={prune() === choice.prune}
                      onChange={() => setPrune(choice.prune)}
                    />{" "}
                    {choice.label}
                  </label>
                )}
              </For>
            </fieldset>
          </Match>
          <Match when={props.kind === "pull"}>
            <fieldset>
              <legend>{strings.sync.strategy}</legend>
              <For each={STRATEGIES}>
                {(choice) => (
                  <label class="choice">
                    <input
                      type="radio"
                      name="strategy"
                      checked={strategy() === choice.strategy}
                      onChange={() => setStrategy(choice.strategy)}
                    />{" "}
                    {choice.label}
                  </label>
                )}
              </For>
            </fieldset>
            <p class="hint">{strings.groupOps.pullsOwnBranch}</p>
          </Match>
          <Match when={props.kind === "push"}>
            <label class="choice">
              <input
                type="checkbox"
                checked={setUpstream()}
                onChange={(event) => setSetUpstream(event.currentTarget.checked)}
              />{" "}
              {strings.groupOps.setUpstream}
            </label>
            <label class="choice">
              <input
                type="checkbox"
                checked={force()}
                onChange={(event) => setForce(event.currentTarget.checked)}
              />{" "}
              {strings.sync.forceWithLease}
            </label>
          </Match>
        </Switch>
        <Actions submit={NAMES[props.kind]} disabled={false} onCancel={() => props.onClose()} />
      </form>
    </Frame>
  );
}
