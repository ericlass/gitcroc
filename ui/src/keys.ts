// The app-wide keyboard shortcuts (UI-22), one place for the handlers, the
// palette's shortcut column and the user documentation (M5-18). "Mod" is
// Cmd on macOS and Ctrl elsewhere. None shadows a CodeMirror editing key:
// keys.test.ts checks them against the keymaps the editors use. Keys that
// belong to one place stay with it: Shift+F10 and the context menu key for
// menus, Ctrl/Cmd+Enter in the commit box, the arrow keys in lists and
// menus, the splitters' keys, the conflict editor's Alt keys.
import { createSignal } from "solid-js";
import type { PaletteOp } from "./ipc";
import { strings } from "./strings/en";

/** A key, as `KeyboardEvent.key` names it, with the modifiers held. */
export interface Keys {
  key: string;
  mod?: boolean;
  shift?: boolean;
  alt?: boolean;
}

export const KEYMAP = {
  palette: { key: "k", mod: true },
  nextRegion: { key: "F6" },
  previousRegion: { key: "F6", shift: true },
  settings: { key: ",", mod: true },
  nextTab: { key: "PageDown", mod: true },
  previousTab: { key: "PageUp", mod: true },
  moveTabRight: { key: "PageDown", mod: true, shift: true },
  moveTabLeft: { key: "PageUp", mod: true, shift: true },
  nextHunk: { key: "F7" },
  previousHunk: { key: "F7", shift: true },
} as const satisfies Record<string, Keys>;

export type Shortcut = keyof typeof KEYMAP;

/** The shortcut of each palette operation that has one, shown in its row (M5-08). */
export const PALETTE_SHORTCUTS: Partial<Record<PaletteOp, Shortcut>> = {
  settings: "settings",
  next_tab: "nextTab",
  previous_tab: "previousTab",
};

/** Whether the app runs on macOS, where Mod is Cmd; set once the core says (App.tsx). */
export const [isMac, setMac] = createSignal(false);

/** `key` as `KeyboardEvent.key` gives it without Shift: letters in lower case. */
const plain = (key: string) => (key.length === 1 ? key.toLowerCase() : key);

/** Whether `event` is `keys`, with no other modifier held. */
export function isKeys(keys: Keys, event: KeyboardEvent, mac = isMac()): boolean {
  const mod = mac ? event.metaKey : event.ctrlKey;
  const other = mac ? event.ctrlKey : event.metaKey;
  return (
    plain(event.key) === plain(keys.key) &&
    mod === (keys.mod ?? false) &&
    !other &&
    event.shiftKey === (keys.shift ?? false) &&
    event.altKey === (keys.alt ?? false)
  );
}

/** Whether `event` is shortcut `id`. */
export const matches = (id: Shortcut, event: KeyboardEvent, mac = isMac()) => isKeys(KEYMAP[id], event, mac);

/** How `keys` are written on the platform, as "Ctrl+Shift+Page Down". */
export function keysLabel(keys: Keys, mac = isMac()): string {
  const names = strings.keys;
  const parts = [
    ...(keys.mod ? [mac ? names.cmd : names.ctrl] : []),
    ...(keys.shift ? [names.shift] : []),
    ...(keys.alt ? [mac ? names.option : names.alt] : []),
    names.names[keys.key] ?? (keys.key.length === 1 ? keys.key.toUpperCase() : keys.key),
  ];
  return names.join(parts);
}

/** How shortcut `id` is written on the platform. */
export const shortcutLabel = (id: Shortcut, mac = isMac()) => keysLabel(KEYMAP[id], mac);

/** Whether a modal dialog is open; the app-wide shortcuts but the palette's wait while one is. */
export const modalOpen = () => document.querySelector("[aria-modal=true]") !== null;

/**
 * The arrow keys, Home and End of a menu (`role=menu`): they move the focus
 * between its enabled items, wrapping around. Tab and Enter work as on any
 * button.
 */
export function onMenuKey(event: KeyboardEvent) {
  const menu = event.currentTarget as HTMLElement;
  const items = [...menu.querySelectorAll<HTMLElement>("[role^=menuitem]:not(:disabled)")];
  const at = items.indexOf(document.activeElement as HTMLElement);
  const to = (() => {
    switch (event.key) {
      case "ArrowDown":
        return at < 0 ? 0 : (at + 1) % items.length;
      case "ArrowUp":
        return at < 0 ? items.length - 1 : (at - 1 + items.length) % items.length;
      case "Home":
        return 0;
      case "End":
        return items.length - 1;
      default:
        return null;
    }
  })();
  if (to === null || items.length === 0) {
    return;
  }
  event.preventDefault();
  items[to]?.focus();
}
