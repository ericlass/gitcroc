import { afterEach, expect, test } from "vitest";
import { emitWatchWarning, fakeState, reset } from "../ipc/fake";
import { createWatchStore } from "./watch";

afterEach(reset);

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const id = () => fakeState.workspace.id;

test("OPS-7 window focus reads the open workspace's repos again", async () => {
  const watch = createWatchStore();
  const unsubscribe = await watch.subscribe(id());
  window.dispatchEvent(new Event("focus"));
  await settle();
  expect(fakeState.refreshes).toEqual([id()]);

  // Closed: focus no longer reads it.
  unsubscribe();
  window.dispatchEvent(new Event("focus"));
  await settle();
  expect(fakeState.refreshes).toEqual([id()]);
});

test("OPS-7 a watcher failure is shown once, also when given before the window listened", async () => {
  fakeState.watchWarning = { workspace_id: id(), message: "OS file watch limit reached." };
  const watch = createWatchStore();
  const unsubscribe = await watch.subscribe(id());
  expect(watch.watch(id()).warning?.message).toBe("OS file watch limit reached.");

  watch.dismiss(id());
  emitWatchWarning({ workspace_id: id(), message: "again" });
  expect(watch.watch(id()).dismissed).toBe(true);
  expect(watch.watch(id()).warning?.message).toBe("OS file watch limit reached.");

  // Another workspace's warning is its own.
  emitWatchWarning({ workspace_id: "elsewhere", message: "other" });
  expect(watch.watch("elsewhere").warning).toBeNull();
  unsubscribe();
});
