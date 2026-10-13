// The steps of the start, for `just bench-ui --verbose` (M1-15), in
// `vite build --mode perf` only. It measures; it changes nothing the app does.

import { core } from "../ipc";

/**
 * Marks the first script, and the first call and answer of each command, as
 * `performance.mark` entries. Runs before the app renders, so it sees the
 * start's calls.
 */
export function markStart() {
  performance.mark("first script");
  // The first frames, and when the tree is first in the DOM, frames aside.
  let frames = 0;
  const onFrame = () => {
    frames++;
    performance.mark(`frame ${frames}`);
    if (frames < 12) {
      requestAnimationFrame(onFrame);
  // Stalls of the main thread over 15 ms in the first 3 s: a task queued
  // back to back that ran late.
  const channel = new MessageChannel();
  let last = performance.now();
  channel.port1.onmessage = () => {
    const now = performance.now();
    if (now - last > 15) {
      performance.mark(`main thread stalled ${Math.round(now - last)} ms`, { startTime: last });
    }
    last = now;
    if (now < 3000) {
      channel.port2.postMessage(null);
    }
  };
  channel.port2.postMessage(null);
    }
  };
  requestAnimationFrame(onFrame);
  // Stalls of the main thread over 15 ms in the first 3 s: a task queued
  // back to back that ran late.
  const channel = new MessageChannel();
  let last = performance.now();
  channel.port1.onmessage = () => {
    const now = performance.now();
    if (now - last > 15) {
      performance.mark(`main thread stalled ${Math.round(now - last)} ms`, { startTime: last });
    }
    last = now;
    if (now < 3000) {
      channel.port2.postMessage(null);
    }
  };
  channel.port2.postMessage(null);
  const observer = new MutationObserver(() => {
    if (document.querySelector('[role="tree"] .tree-repo .tree-head') !== null) {
      performance.mark("tree in the DOM");
      observer.disconnect();
    }
  });
  observer.observe(document, { childList: true, subtree: true });
  const commands = core.commands as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>;
  for (const [name, command] of Object.entries(commands)) {
    let first = true;
    commands[name] = (...args) => {
      const answer = command(...args);
      if (first) {
        first = false;
        performance.mark(`send ${name}`);
        void answer.finally(() => performance.mark(`answer ${name}`));
      }
      return answer;
    };
  }
}

/** The marks so far and the page's load steps, in milliseconds since the Unix epoch. */
export function startMarks(): { name: string; unix_ms: number }[] {
  const at = (ms: number) => performance.timeOrigin + ms;
  const marks = performance.getEntriesByType("mark").map((mark) => ({ name: mark.name, unix_ms: at(mark.startTime) }));
  const nav = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined;
  if (nav !== undefined) {
    for (const step of ["responseEnd", "domInteractive", "domContentLoadedEventEnd", "loadEventEnd"] as const) {
      marks.push({ name: `page ${step}`, unix_ms: at(nav[step]) });
    }
  }
  marks.push({ name: "page timeOrigin", unix_ms: performance.timeOrigin });
  for (const entry of performance.getEntriesByType("resource") as PerformanceResourceTiming[]) {
    if (/\/assets\/index-[^/]*\.(js|css)$/.test(entry.name)) {
      const kind = entry.name.endsWith(".js") ? "script" : "styles";
      marks.push({ name: `${kind} requested`, unix_ms: at(entry.startTime) });
      marks.push({ name: `${kind} loaded`, unix_ms: at(entry.responseEnd) });
    }
  }
  return marks;
}
