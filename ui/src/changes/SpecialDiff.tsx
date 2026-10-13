// Diffs that are not drawn as text (UI-18): binary files with their sizes,
// images before and after, a diff over the line limit until the user loads
// it, an untracked folder and a submodule's commits.
import { createResource, Match, onCleanup, Show, Switch } from "solid-js";
import { diffImage, type FileDiff, type FileRef, type Image, type ImageFormat, type WorkspaceId } from "../ipc";

const MIME: Record<ImageFormat, string> = {
  png: "image/png",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  bmp: "image/bmp",
};

/** A size in bytes for people: `68 B`, `1.2 KB`, `3.4 MB`. */
export function formatSize(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(1)} ${units[unit]}`;
}

const sizeText = (size: number | null) => (size === null ? "none" : formatSize(size));

/** The bytes of a base64 text. */
function bytesOf(base64: string): Uint8Array<ArrayBuffer> {
  const text = atob(base64);
  const bytes = new Uint8Array(text.length);
  for (let index = 0; index < text.length; index++) {
    bytes[index] = text.charCodeAt(index);
  }
  return bytes;
}

/** One side of an image diff, shown through a `blob:` URL (SEC-2). */
function ImageSide(props: { label: string; image: Image | null; load: () => Promise<string | null> }) {
  /** The URL made last; each new one replaces it. */
  let made: string | null = null;
  const replace = (next: string | null) => {
    if (made !== null) {
      URL.revokeObjectURL(made);
    }
    made = next;
    return next;
  };
  const [url] = createResource(
    () => props.image,
    async (image) => {
      const base64 = await props.load();
      return replace(
        base64 === null ? null : URL.createObjectURL(new Blob([bytesOf(base64)], { type: MIME[image.format] })),
      );
    },
  );
  onCleanup(() => replace(null));
  return (
    <figure class="diff-image">
      <figcaption>
        {props.label}: {props.image === null ? "none" : `${props.image.format.toUpperCase()}, ${formatSize(props.image.size)}`}
      </figcaption>
      <Show when={props.image !== null && url()}>{(src) => <img src={src()} alt={`${props.label} image`} />}</Show>
    </figure>
  );
}

export default function SpecialDiff(props: {
  diff: Exclude<FileDiff, { kind: "text" }>;
  workspace: WorkspaceId;
  handle: number;
  file: FileRef;
  onLoadAnyway: () => void;
}) {
  return (
    <div class="diff-special">
      <Switch>
        <Match when={props.diff.kind === "binary" && props.diff}>
          {(diff) => (
            <p>
              Binary file changed. Before: {sizeText(diff().old_size)}, after: {sizeText(diff().new_size)}.
            </p>
          )}
        </Match>
        <Match when={props.diff.kind === "image" && props.diff}>
          {(diff) => (
            <div class="diff-images">
              <ImageSide
                label="Before"
                image={diff().old}
                load={() => diffImage(props.workspace, props.handle, props.file, "old")}
              />
              <ImageSide
                label="After"
                image={diff().new}
                load={() => diffImage(props.workspace, props.handle, props.file, "new")}
              />
            </div>
          )}
        </Match>
        <Match when={props.diff.kind === "too_large" && props.diff}>
          {(diff) => (
            <div class="diff-large">
              <p>
                This diff is large: {diff().added.toLocaleString()} lines added and {diff().removed.toLocaleString()}{" "}
                removed. Showing it may take a while.
              </p>
              <button type="button" onClick={() => props.onLoadAnyway()}>
                Load diff
              </button>
            </div>
          )}
        </Match>
        <Match when={props.diff.kind === "directory"}>
          <p>An untracked folder. Its files are not read.</p>
        </Match>
        <Match when={props.diff.kind === "submodule" && props.diff}>
          {(diff) => (
            <dl class="diff-submodule">
              <dt>Submodule commit before</dt>
              <dd class="hash">{diff().old ?? "none"}</dd>
              <dt>Submodule commit after</dt>
              <dd class="hash">{diff().new ?? "none"}</dd>
            </dl>
          )}
        </Match>
      </Switch>
    </div>
  );
}
