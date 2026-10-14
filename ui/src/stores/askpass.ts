// Git's prompts waiting for the user (D10), oldest first, as the core
// sends them. The store keeps the questions only: an answer goes straight
// to the core and is not kept (SEC-1).
import { createStore } from "solid-js/store";
import { type AskpassPrompt, askpassAnswer, core } from "../ipc";

export function createAskpassStore() {
  const [prompts, setPrompts] = createStore<AskpassPrompt[]>([]);

  const remove = (promptId: number) => setPrompts((open) => open.filter((prompt) => prompt.prompt_id !== promptId));

  /** The prompt the dialog shows: the oldest one open. */
  const current = () => prompts[0];

  /** Sends `answer` to git, or no answer when it is `null`. */
  const answer = async (promptId: number, answer: string | null) => {
    remove(promptId);
    await askpassAnswer(promptId, answer);
  };

  /** Starts listening for prompts; resolves to the unsubscribe function. */
  const subscribe = async () => {
    const stops = await Promise.all([
      core.events.askpassPrompt.listen((event) => setPrompts(prompts.length, event.payload)),
      core.events.askpassClosed.listen((event) => remove(event.payload.prompt_id)),
    ]);
    return () => {
      for (const stop of stops) {
        stop();
      }
    };
  };

  return { prompts, current, answer, subscribe };
}

export type AskpassStore = ReturnType<typeof createAskpassStore>;
