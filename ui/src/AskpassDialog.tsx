// Git's prompt that it cannot answer silently (D10): a credential, or an
// SSH host key question. It names the job and the repo; the prompt is
// shown as text (SEC-3), and a password or passphrase is masked. Escape
// and Cancel give git no answer.
import { createSignal, Match, onMount, Switch } from "solid-js";
import type { AskpassPrompt } from "./ipc";
import { strings } from "./strings/en";

export default function AskpassDialog(props: {
  prompt: AskpassPrompt;
  onAnswer: (answer: string | null) => void;
}) {
  let dialog: HTMLDivElement | undefined;
  const [value, setValue] = createSignal("");

  onMount(() => dialog?.querySelector<HTMLElement>("input, button")?.focus());

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      props.onAnswer(null);
    }
  };

  const where = () => {
    const repos = props.prompt.repo_names.join(", ");
    return repos === "" ? props.prompt.workspace_name : strings.askpass.where(repos, props.prompt.workspace_name);
  };

  return (
    <div class="backdrop">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="askpass-title"
        aria-describedby="askpass-prompt"
        class="dialog askpass"
        ref={(element) => (dialog = element)}
        onKeyDown={onKeyDown}
      >
        <h2 id="askpass-title">
          {props.prompt.job_label}: {where()}
        </h2>
        <p id="askpass-prompt" class="askpass-prompt">
          {props.prompt.text}
        </p>
        <Switch>
          <Match when={props.prompt.kind === "yes_no"}>
            <div class="dialog-actions">
              <button type="button" onClick={() => props.onAnswer("yes")}>
                {strings.common.yes}
              </button>
              <button type="button" onClick={() => props.onAnswer("no")}>
                {strings.common.no}
              </button>
            </div>
          </Match>
          <Match when={true}>
            <form
              onSubmit={(event) => {
                event.preventDefault();
                props.onAnswer(value());
              }}
            >
              <input
                type={props.prompt.kind === "secret" ? "password" : "text"}
                aria-labelledby="askpass-prompt"
                autocomplete="off"
                spellcheck={false}
                value={value()}
                onInput={(event) => setValue(event.currentTarget.value)}
              />
              <div class="dialog-actions">
                <button type="button" onClick={() => props.onAnswer(null)}>
                  {strings.common.cancel}
                </button>
                <button type="submit">{strings.common.ok}</button>
              </div>
            </form>
          </Match>
        </Switch>
      </div>
    </div>
  );
}
