// One row's graph cell, drawn from the lanes the core computed (ARCH-6):
// the UI only turns edges into lines. Colors are theme tokens (UI-23).
import { For, Show } from "solid-js";
import type { Edge, GraphRow } from "../ipc";

/** Horizontal space per lane, in CSS pixels. */
export const LANE_WIDTH = 12;

/** How many line colors the theme defines (`--graph-0` and up). */
export const GRAPH_COLORS = 8;

const x = (lane: number) => LANE_WIDTH / 2 + lane * LANE_WIDTH;

const colorClass = (color: number) => `graph-c${color % GRAPH_COLORS}`;

export default function Graph(props: { graph: GraphRow | null; height: number }) {
  const middle = () => props.height / 2;
  const line = (edge: Edge, top: number, bottom: number) => (
    <line
      class={`graph-line ${colorClass(edge.color)}`}
      x1={x(edge.from)}
      y1={top}
      x2={x(edge.to)}
      y2={bottom}
    />
  );
  return (
    <Show when={props.graph}>
      {(graph) => (
        <svg class="graph" height={props.height} aria-hidden="true">
          <For each={graph().up}>{(edge) => line(edge, 0, middle())}</For>
          <For each={graph().down}>{(edge) => line(edge, middle(), props.height)}</For>
          <circle class={`graph-dot ${colorClass(graph().color)}`} cx={x(graph().lane)} cy={middle()} r={4} />
        </svg>
      )}
    </Show>
  );
}
