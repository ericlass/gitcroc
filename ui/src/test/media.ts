// A controllable `window.matchMedia` for jsdom, which has none. Only the dark
// color scheme query ever matches, and only after `setSystemDark(true)`.
import { DARK_QUERY } from "../theme/theme";

let systemDark = false;
const listeners = new Set<(event: MediaQueryListEvent) => void>();

function mediaQueryList(query: string): MediaQueryList {
  return {
    media: query,
    get matches() {
      return query === DARK_QUERY && systemDark;
    },
    onchange: null,
    addEventListener: (_type: string, listener: (event: MediaQueryListEvent) => void) => {
      if (query === DARK_QUERY) {
        listeners.add(listener);
      }
    },
    removeEventListener: (_type: string, listener: (event: MediaQueryListEvent) => void) => {
      listeners.delete(listener);
    },
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
  } as MediaQueryList;
}

/** Changes the OS color scheme, notifying listeners as a browser would. */
export function setSystemDark(dark: boolean): void {
  systemDark = dark;
  for (const listener of [...listeners]) {
    listener({ matches: dark, media: DARK_QUERY } as MediaQueryListEvent);
  }
}

export function installMatchMedia(): void {
  systemDark = false;
  listeners.clear();
  window.matchMedia = mediaQueryList;
}
