// F6 and Shift+F6 (UI-22): the focus moves to the next or previous region
// of the window, in this order, wrapping around, skipping the regions not
// shown. In a region the focus lands on its list, whose selected row is the
// active one, or else on its first control.

/** The regions, by the element that holds each, and the list the focus prefers in it. */
const REGIONS: { region: string; list?: string }[] = [
  { region: ".tab-bar", list: "[role=tab][aria-selected=true]" },
  { region: ".toolbar" },
  { region: ".pane-tree", list: "[role=tree]" },
  { region: ".pane-history", list: "[role=listbox]" },
  { region: ".pane-changes", list: "[role=treegrid]" },
  { region: ".status-bar" },
];

const FOCUSABLE =
  "button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], " +
  "[tabindex]:not([tabindex='-1'])";

const shown = (element: HTMLElement) => element.getClientRects().length > 0;

/** Where the focus goes in `region`, or `null` when nothing there takes it. */
function targetIn(region: HTMLElement, list: string | undefined): HTMLElement | null {
  const preferred = list === undefined ? null : region.querySelector<HTMLElement>(list);
  if (preferred !== null && shown(preferred)) {
    return preferred;
  }
  return [...region.querySelectorAll<HTMLElement>(FOCUSABLE)].find(shown) ?? null;
}

/** Moves the focus to the region `step` places from the one that has it; from none, to the first or the last. */
export function moveToRegion(step: 1 | -1, root: ParentNode = document) {
  const regions = REGIONS.flatMap(({ region, list }) => {
    const element = root.querySelector<HTMLElement>(region);
    const target = element === null ? null : targetIn(element, list);
    return element === null || target === null ? [] : [{ element, target }];
  });
  if (regions.length === 0) {
    return;
  }
  const focused = document.activeElement;
  const at = regions.findIndex(({ element }) => focused !== null && element.contains(focused));
  const next = at < 0 ? (step === 1 ? 0 : regions.length - 1) : (at + step + regions.length) % regions.length;
  regions[next]?.target.focus();
}
