# M1-15 Cold start: investigation

## Goal
Find out whether the cold start can reach 1 s (section 9 "Cold start: tree visible from cache"), and how. M1-12 measured 1140 ms median (p95 1215 ms) pinned to 8 cores. Of that, 555 to 950 ms passes from the start of `run()` to the page's start, while Tauri creates the window and WebKitGTK starts its web and network processes. Loading the page takes about 65 ms, and the page needs about 270 ms more to show the tree. The core's part (`cold-start-core`) is 2 ms.

This is an investigation: it delivers findings and a recommendation, and code only where a fix is small and clearly right.

## Depends on
M1-12

## Spec
Respects: PERF-3, ARCH-6, ARCH-7, section 9 budgets, D2

## In scope
- Measure the phases of the start more finely, with `just bench-ui --verbose` extended as needed. Phases: process start to window creation, window creation to the web process running, page request to the first script, the first script to the first IPC answers, and those answers to the tree's first frame.
- For each phase, try what could shorten it and measure the effect. Examples:
  - Tauri and wry window options;
  - the WebKitGTK sandbox and its process launch (bubblewrap), the GPU process, and the hardware acceleration policy;
  - showing the window only once the page is ready, against showing it early;
  - a smaller first script, with code split so CodeMirror and the changes pane load later;
  - fewer or merged IPC calls before the tree's first paint;
  - running the start's IPC calls in parallel instead of one after another.
- Compare with a bare Tauri 2 app on the same machine. That gives the floor the webview itself sets.
- Write the findings into `docs/tasks/M1-signoff.md` under the cold start row. Recommend one of:
  - fixes that reach 1 s, each as its own task;
  - a section 9 change by explicit decision, with the measured floor;
  - revisiting D2 (section 13).

## Out of scope
- Changing D2 or section 9 itself: that is a decision for Eric, which this task only prepares.
- Input latency (M1-13) and memory (M1-14).

## Acceptance
- The sign-off's cold start row has the phase breakdown, what was tried with its effect, the floor of a bare Tauri app, and a recommendation.
- Any code change keeps `just check` green and does not make another `just bench-ui` row worse than its budget.
