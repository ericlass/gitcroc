import { render } from "solid-js/web";
import App from "./App";
import { markStart } from "./perf/marks";
import { applyInitialTheme } from "./theme/theme";
import "./theme/tokens.css";
import "./App.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("index.html has no #root element");
}
// A build-time constant: only `just bench-ui` builds mark the start.
if (__PERF_TRACE__) {
  markStart();
}
applyInitialTheme();
// A build-time constant: only `just bench-ui` builds carry the recorder.
if (__PERF_TRACE__) {
  void import("./perf/trace").then((trace) => trace.install());
}
render(() => <App />, root);
if (__PERF_TRACE__) {
  performance.mark("rendered");
}
