import { defaultKeymap, historyKeymap } from "@codemirror/commands";
import type { KeyBinding } from "@codemirror/view";
import { expect, test } from "vitest";
import { conflictKeys } from "./conflict/editor";
import { PALETTE_CATALOG } from "./ipc";
import { isKeys, KEYMAP, keysLabel, matches, PALETTE_SHORTCUTS, type Shortcut, shortcutLabel } from "./keys";
import { strings } from "./strings/en";

const SHORTCUTS = Object.keys(KEYMAP) as Shortcut[];

/** The event `keys` (KeyboardEvent.key with the modifiers held) would give. */
const event = (key: string, held: { ctrl?: boolean; meta?: boolean; shift?: boolean; alt?: boolean } = {}) =>
  ({
    key,
    ctrlKey: held.ctrl ?? false,
    metaKey: held.meta ?? false,
    shiftKey: held.shift ?? false,
    altKey: held.alt ?? false,
  }) as KeyboardEvent;

test("UI-22 keys.ts has no two shortcuts on the same keys", () => {
  for (const mac of [false, true]) {
    const written = SHORTCUTS.map((id) => shortcutLabel(id, mac));
    expect(new Set(written).size, written.join(", ")).toBe(SHORTCUTS.length);
  }
  // And each matches its own event only.
  for (const id of SHORTCUTS) {
    const keys = KEYMAP[id];
    const own = event(keys.key, { ctrl: "mod" in keys, shift: "shift" in keys });
    expect(SHORTCUTS.filter((other) => matches(other, own, false))).toEqual([id]);
  }
});

test("UI-22 each shortcut has a label in the strings module", () => {
  const labels: Record<string, string> = strings.keys.shortcuts;
  expect(Object.keys(labels).sort()).toEqual([...SHORTCUTS].sort());
  for (const id of SHORTCUTS) {
    expect(labels[id]?.length).toBeGreaterThan(0);
  }
  // The palette's operations with a shortcut are the catalog's.
  const ops = PALETTE_CATALOG.map((known) => known.op as string);
  expect(Object.keys(PALETTE_SHORTCUTS).filter((op) => !ops.includes(op))).toEqual([]);
});

test("UI-22 shortcuts are written with the platform's modifier and match only it", () => {
  expect(shortcutLabel("palette", false)).toBe("Ctrl+K");
  expect(shortcutLabel("palette", true)).toBe("Cmd+K");
  expect(shortcutLabel("moveTabRight", false)).toBe("Ctrl+Shift+Page Down");
  expect(shortcutLabel("previousHunk", true)).toBe("Shift+F7");
  expect(keysLabel({ key: "1", alt: true }, true)).toBe("Option+1");
  expect(matches("palette", event("k", { ctrl: true }), false)).toBe(true);
  expect(matches("palette", event("K", { ctrl: true }), false)).toBe(true);
  expect(matches("palette", event("k", { meta: true }), false)).toBe(false);
  expect(matches("palette", event("k", { meta: true }), true)).toBe(true);
  expect(matches("palette", event("k", { ctrl: true }), true)).toBe(false);
  expect(matches("palette", event("k", { ctrl: true, alt: true }), false)).toBe(false);
  expect(matches("nextTab", event("PageDown", { ctrl: true, shift: true }), false)).toBe(false);
});

/** The modifiers and key of a CodeMirror key name such as `Shift-Mod-k`, on the platform. */
function parse(name: string, mac: boolean, shift = false) {
  const parts = name.split(/-(?!$)/);
  const key = parts.pop() ?? "";
  const held = { ctrl: false, meta: false, shift, alt: false };
  for (const part of parts) {
    if (part === "Mod") {
      held[mac ? "meta" : "ctrl"] = true;
    } else if (part === "Cmd" || part === "Meta") {
      held.meta = true;
    } else if (part === "Ctrl") {
      held.ctrl = true;
    } else if (part === "Shift") {
      held.shift = true;
    } else if (part === "Alt") {
      held.alt = true;
    }
  }
  return event(key, held);
}

/** Every key a binding takes on the platform, with its Shift form when it has one. */
function keysOf(binding: KeyBinding, mac: boolean) {
  const name = (mac ? binding.mac : binding.linux ?? binding.win) ?? binding.key;
  if (name === undefined) {
    return [];
  }
  return binding.shift === undefined ? [parse(name, mac)] : [parse(name, mac), parse(name, mac, true)];
}

test("UI-22 no shortcut shadows a CodeMirror key of the diff or the conflict editor", () => {
  const editors = [...defaultKeymap, ...historyKeymap, ...conflictKeys];
  expect(editors.length).toBeGreaterThan(50);
  for (const mac of [false, true]) {
    for (const binding of editors) {
      for (const pressed of keysOf(binding, mac)) {
        const shadowed = SHORTCUTS.filter((id) => isKeys(KEYMAP[id], pressed, mac));
        expect(shadowed, `${binding.key ?? binding.mac} on ${mac ? "macOS" : "Linux and Windows"}`).toEqual([]);
      }
    }
  }
});
