// One row's graph cell, drawn from the lanes the core computed (ARCH-6):
// the UI only turns edges into lines. Colors are theme tokens (UI-23).
import { Index, Show } from "solid-js";
import type { Edge, GraphRow } from "../ipc";

/** Horizontal space per lane, in CSS pixels. */
export const LANE_WIDTH = 12;

/** How many line colors the theme defines (`--graph-0` and up). */
export const GRAPH_COLORS = 8;

const x = (lane: number) => LANE_WIDTH / 2 + lane * LANE_WIDTH;

const colorClass = (color: number) => `graph-c${color % GRAPH_COLORS}`;

/** One line of the cell; it is kept as the row shows other commits (M1-14). */
function Line(props: { edge: Edge; top: number; bottom: number }) {
  return (
    <line
      class={`graph-line ${colorClass(props.edge.color)}`}
      x1={x(props.edge.from)}
      y1={props.top}
      x2={x(props.edge.to)}
      y2={props.bottom}
    />
  );
}

export default function Graph(props: { graph: GraphRow | null; height: number }) {
  const middle = () => props.height / 2;
  return (
    <Show when={props.graph}>
      {(graph) => (
        <svg class="graph" height={props.height} aria-hidden="true">
          <Index each={graph().up}>{(edge) => <Line edge={edge()} top={0} bottom={middle()} />}</Index>
          <Index each={graph().down}>{(edge) => <Line edge={edge()} top={middle()} bottom={props.height} />}</Index>
          <circle class={`graph-dot ${colorClass(graph().color)}`} cx={x(graph().lane)} cy={middle()} r={4} />
        </svg>
      )}
    </Show>
  );
}
