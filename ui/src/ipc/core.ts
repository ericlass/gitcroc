// The interface the UI talks to: the generated commands and the listening half
// of the generated events. `real` is the Tauri backend; `fake.ts` implements
// the same interface for front-end tests.
import { commands, events } from "./bindings";

export type Commands = typeof commands;

type Listenable<E> = Pick<E, Extract<keyof E, "listen" | "once">>;

export type Events = { [K in keyof typeof events]: Listenable<(typeof events)[K]> };

export interface Core {
  commands: Commands;
  events: Events;
}

export const real: Core = { commands, events };
