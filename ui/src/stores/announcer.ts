// The window's one polite live region (M5-10): what a screen reader says
// without the focus moving. A job that ended failed or in conflict is
// announced here, and later notices, such as the update notice (M5-12), use
// it too. Progress is never announced, so a running job does not talk on
// every step.
import { createSignal } from "solid-js";

export function createAnnouncer() {
  const [text, setText] = createSignal("");
  return {
    /** What the live region holds. */
    text,
    /** Says `message`; the same message again is said again. */
    announce: (message: string) => setText((said) => (said === message ? `${message}\u00a0` : message)),
  };
}

export type Announcer = ReturnType<typeof createAnnouncer>;
