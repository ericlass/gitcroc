// The window's title bar (UI-35). Unless the user chose the system's, the tab bar is the title bar: its empty space
// moves the window, a double-click on it maximizes or restores the window, and on Windows and Linux it ends with the
// window buttons; on macOS the system's buttons stay, over its left end. Screens without a tab bar show `TitleBar`,
// with the app's name. The buttons are left out of the tab order, as the system's are. The core moves, sizes and
// closes the window; this sends what the user did.
import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { type WindowAction, windowControl, windowMaximized } from "../ipc";
import { strings } from "../strings/en";

export interface WindowChrome {
  /** The app draws the title bar: the system's is hidden or, on macOS, transparent over the page. */
  own: () => boolean;
  /** The app's own window buttons are shown (Windows and Linux). */
  buttons: () => boolean;
  /** The system's buttons sit over the bar's left end (macOS). */
  trafficLights: () => boolean;
  maximized: () => boolean;
  control: (action: WindowAction) => void;
  /** A press on the bar's empty space: moves the window, or with a double-click maximizes or restores it. */
  onPress: (event: MouseEvent) => void;
}

/** The title bar on `os`, as `systemTitleBar` says; nothing is drawn until both are known. */
export function createWindowChrome(os: () => string | undefined, systemTitleBar: () => boolean | null): WindowChrome {
  const [maximized, setMaximized] = createSignal(false);
  const own = () => os() !== undefined && systemTitleBar() === false;
  const buttons = () => own() && os() !== "macos";
  // A refused action leaves the window as it was, which shows for itself.
  const control = (action: WindowAction) => void windowControl(action).catch(() => undefined);
  const read = () => void windowMaximized().then(setMaximized, () => undefined);
  // The page resizes with the window, also when the system maximizes it.
  createEffect(() => {
    if (!buttons()) {
      return;
    }
    read();
    window.addEventListener("resize", read);
    onCleanup(() => window.removeEventListener("resize", read));
  });
  return {
    own,
    buttons,
    trafficLights: () => own() && os() === "macos",
    maximized,
    control,
    onPress: (event) => {
      // The empty space itself, with the main button: tabs and buttons keep their own presses.
      if (!own() || event.button !== 0 || event.target !== event.currentTarget) {
        return;
      }
      event.preventDefault();
      control(event.detail === 2 ? "toggle_maximize" : "start_dragging");
    },
  };
}

/** Minimize, maximize or restore, and close, on Windows and Linux. */
export function WindowButtons(props: { chrome: WindowChrome }) {
  const glyph = { fill: "none", stroke: "currentColor", "stroke-width": "1" } as const;
  return (
    <Show when={props.chrome.buttons()}>
      <div class="window-buttons" role="group" aria-label={strings.window.buttons}>
        <button
          type="button"
          class="window-button"
          tabindex="-1"
          aria-label={strings.window.minimize}
          title={strings.window.minimize}
          onClick={() => props.chrome.control("minimize")}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" {...glyph}>
            <path d="M0 5.5h10" />
          </svg>
        </button>
        <button
          type="button"
          class="window-button"
          tabindex="-1"
          aria-label={props.chrome.maximized() ? strings.window.restore : strings.window.maximize}
          title={props.chrome.maximized() ? strings.window.restore : strings.window.maximize}
          onClick={() => props.chrome.control("toggle_maximize")}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" {...glyph}>
            <Show when={props.chrome.maximized()} fallback={<rect x="0.5" y="0.5" width="9" height="9" />}>
              <path d="M2.5 2.5v-2h7v7h-2" />
              <rect x="0.5" y="2.5" width="7" height="7" />
            </Show>
          </svg>
        </button>
        <button
          type="button"
          class="window-button window-close"
          tabindex="-1"
          aria-label={strings.window.close}
          title={strings.window.close}
          onClick={() => props.chrome.control("close")}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" {...glyph}>
            <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" />
          </svg>
        </button>
      </div>
    </Show>
  );
}

/** The title bar of a screen without a tab bar: the app's name, the empty space and the window buttons. */
export function TitleBar(props: { chrome: WindowChrome; name: string }) {
  return (
    <Show when={props.chrome.own()}>
      <div
        class="title-bar"
        classList={{ "title-bar-mac": props.chrome.trafficLights() }}
        onMouseDown={(event) => props.chrome.onPress(event)}
      >
        <span class="title-bar-name">{props.name}</span>
        <WindowButtons chrome={props.chrome} />
      </div>
    </Show>
  );
}
