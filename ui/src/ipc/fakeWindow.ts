// The window for the fake core (fake.ts), like the core's (UI-35): what the window buttons and the tab bar's empty
// space ask is recorded, and maximizing flips the state the maximize button shows. Test support only.
import type { IpcError, WindowAction } from "./bindings";

type Result<T> = Promise<{ status: "ok"; data: T } | { status: "error"; error: IpcError }>;

export const fakeWindow: {
  /** Every action asked of the window, oldest first. */
  actions: WindowAction[];
  maximized: boolean;
} = { actions: [], maximized: false };

export function resetWindow(): void {
  fakeWindow.actions = [];
  fakeWindow.maximized = false;
}

export function windowCommands(): {
  windowControl: (action: WindowAction) => Result<null>;
  windowMaximized: () => Result<boolean>;
} {
  return {
    windowControl: (action) => {
      fakeWindow.actions.push(action);
      if (action === "toggle_maximize") {
        fakeWindow.maximized = !fakeWindow.maximized;
        // The page resizes with the window.
        setTimeout(() => window.dispatchEvent(new Event("resize")), 0);
      }
      return Promise.resolve({ status: "ok", data: null });
    },
    windowMaximized: () => Promise.resolve({ status: "ok", data: fakeWindow.maximized }),
  };
}
