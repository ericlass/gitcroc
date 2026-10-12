// A vertical splitter that resizes the pane to its left. Drag it, or focus it
// and use the arrow keys; double-click resets the pane.
import type { ResizablePane } from "../stores/layout";

const KEY_STEP = 16;

export default function Splitter(props: {
  pane: ResizablePane;
  label: string;
  width: number;
  min: number;
  /** The largest width the pane can take now, if known. */
  max: () => number | undefined;
  onResize: (width: number, max: number | undefined) => void;
  onReset: () => void;
}) {
  let drag: { pointerId: number; startX: number; startWidth: number; max: number | undefined } | undefined;

  const onPointerDown = (event: PointerEvent & { currentTarget: HTMLElement }) => {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag = { pointerId: event.pointerId, startX: event.clientX, startWidth: props.width, max: props.max() };
  };

  const onPointerMove = (event: PointerEvent) => {
    if (drag?.pointerId === event.pointerId) {
      props.onResize(drag.startWidth + event.clientX - drag.startX, drag.max);
    }
  };

  const onPointerUp = (event: PointerEvent & { currentTarget: HTMLElement }) => {
    if (drag?.pointerId === event.pointerId) {
      event.currentTarget.releasePointerCapture(event.pointerId);
      drag = undefined;
    }
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const step = event.key === "ArrowLeft" ? -KEY_STEP : event.key === "ArrowRight" ? KEY_STEP : 0;
    if (step !== 0) {
      event.preventDefault();
      props.onResize(props.width + step, props.max());
    }
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={props.label}
      aria-valuenow={props.width}
      aria-valuemin={props.min}
      tabindex="0"
      class="splitter"
      data-pane={props.pane}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onDblClick={() => props.onReset()}
      onKeyDown={onKeyDown}
    />
  );
}
