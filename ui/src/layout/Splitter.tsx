// A splitter that resizes the pane or column beside it. A vertical one
// resizes what is to its left; a horizontal one what is below it. Drag it,
// or focus it and use the arrow keys; double-click resets it.
const KEY_STEP = 16;

export default function Splitter(props: {
  /** What it resizes, for styling: a pane, or a history column. */
  pane: string;
  /** `splitter` (the default) between panes, `column-splitter` in a header. */
  class?: string;
  /** `vertical` (the default) resizes a width, `horizontal` the height of the pane below. */
  orientation?: "vertical" | "horizontal";
  label: string;
  /** The size it controls: a width, or a height when horizontal. */
  width: number;
  min: number;
  /** The largest size the pane can take now, if known. */
  max: () => number | undefined;
  onResize: (width: number, max: number | undefined) => void;
  onReset: () => void;
  /** A resize ended: a drag let go, a key pressed, a reset. */
  onCommit?: () => void;
}) {
  let drag: { pointerId: number; start: number; startWidth: number; max: number | undefined } | undefined;

  const horizontal = () => props.orientation === "horizontal";
  // A horizontal splitter sits above its pane: moving up makes it taller.
  const position = (event: PointerEvent) => (horizontal() ? -event.clientY : event.clientX);

  const onPointerDown = (event: PointerEvent & { currentTarget: HTMLElement }) => {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag = { pointerId: event.pointerId, start: position(event), startWidth: props.width, max: props.max() };
  };

  const onPointerMove = (event: PointerEvent) => {
    if (drag?.pointerId === event.pointerId) {
      props.onResize(drag.startWidth + position(event) - drag.start, drag.max);
    }
  };

  const onPointerUp = (event: PointerEvent & { currentTarget: HTMLElement }) => {
    if (drag?.pointerId === event.pointerId) {
      event.currentTarget.releasePointerCapture(event.pointerId);
      drag = undefined;
      props.onCommit?.();
    }
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const [less, more] = horizontal() ? ["ArrowDown", "ArrowUp"] : ["ArrowLeft", "ArrowRight"];
    const step = event.key === less ? -KEY_STEP : event.key === more ? KEY_STEP : 0;
    if (step !== 0) {
      event.preventDefault();
      props.onResize(props.width + step, props.max());
      props.onCommit?.();
    }
  };

  return (
    <div
      role="separator"
      aria-orientation={horizontal() ? "horizontal" : "vertical"}
      aria-label={props.label}
      aria-valuenow={props.width}
      aria-valuemin={props.min}
      tabindex="0"
      class={`${props.class ?? "splitter"}${horizontal() ? " splitter-horizontal" : ""}`}
      data-pane={props.pane}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onDblClick={() => {
        props.onReset();
        props.onCommit?.();
      }}
      onKeyDown={onKeyDown}
    />
  );
}
