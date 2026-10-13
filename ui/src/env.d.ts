/** True in Vitest and in `vite build --mode fake`: use src/ipc/fake.ts. */
declare const __FAKE_CORE__: boolean;

/** True in `vite build --mode perf`, which `just bench-ui` runs: load src/perf/trace.ts. */
declare const __PERF_TRACE__: boolean;
