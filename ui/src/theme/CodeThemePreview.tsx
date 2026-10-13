// A few lines of a diff in one code theme (UI-24), for the Settings dialog.
// The theme's variables are set on the preview alone, so it can show a
// theme other than the one in use.
import { createEffect } from "solid-js";
import type { CodeTheme } from "../ipc";
import { applyCodeTheme } from "./codeThemes";

export default function CodeThemePreview(props: { theme: CodeTheme }) {
  let element: HTMLDivElement | undefined;
  createEffect(() => {
    if (element) {
      applyCodeTheme(element, props.theme);
    }
  });
  return (
    <div class="code-preview" data-theme-name={props.theme} aria-hidden="true" ref={(el) => (element = el)}>
      <div class="code-preview-hunk">@@ -1,4 +1,4 @@</div>
      <div class="code-preview-line">
        <span class="tok-keyword">function</span> <span class="tok-function">greet</span>(name:{" "}
        <span class="tok-type">string</span>) {"{"}
      </div>
      <div class="code-preview-line removed">
        {"  "}
        <span class="tok-keyword">return</span> <span class="tok-string">"Hello "</span> + name;{" "}
        <span class="tok-comment">// old</span>
      </div>
      <div class="code-preview-line added">
        {"  "}
        <span class="tok-keyword">return</span> <span class="tok-string">"Hello, "</span> + name +{" "}
        <span class="tok-number">1</span>;
      </div>
      <div class="code-preview-line">{"}"}</div>
    </div>
  );
}
