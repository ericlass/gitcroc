import { render } from "solid-js/web";
import App from "./App";
import { applyInitialTheme } from "./theme/theme";
import "./theme/tokens.css";
import "./App.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("index.html has no #root element");
}
applyInitialTheme();
render(() => <App />, root);
