// Recursive pane tree renderer.
//
// The backend owns the tree; this module only projects a PaneNode tree onto
// DOM. Existing xterm host elements are re-parented (never re-created) so
// terminal state survives layout changes within a workspace.

import type { Direction, PaneNode } from "./types";
import { clamp } from "./util";

export interface RenderCtx {
  getPaneEl(paneId: string): HTMLElement;
  /** Commit a divider drag to the backend; returns the clamped ratio. */
  commitRatio(splitId: string, ratio: number): Promise<number>;
}

export function renderTree(container: HTMLElement, node: PaneNode, ctx: RenderCtx): void {
  container.replaceChildren(buildNode(node, ctx));
}

function buildNode(node: PaneNode, ctx: RenderCtx): HTMLElement {
  if (node.kind === "pane") {
    const cell = document.createElement("div");
    cell.className = "pane-cell";
    cell.appendChild(ctx.getPaneEl(node.id));
    return cell;
  }

  const split = document.createElement("div");
  split.className = `split ${node.direction}`;
  const first = document.createElement("div");
  first.className = "split-child";
  const second = document.createElement("div");
  second.className = "split-child";
  applyRatio(first, second, node.ratio);
  first.appendChild(buildNode(node.first, ctx));
  second.appendChild(buildNode(node.second, ctx));

  const divider = document.createElement("div");
  divider.className = "divider";
  attachDrag(divider, split, first, second, node.direction, node, ctx);

  split.append(first, divider, second);
  return split;
}

function applyRatio(first: HTMLElement, second: HTMLElement, ratio: number): void {
  first.style.flex = `${ratio} 1 0`;
  second.style.flex = `${1 - ratio} 1 0`;
}

function attachDrag(
  divider: HTMLElement,
  split: HTMLElement,
  first: HTMLElement,
  second: HTMLElement,
  direction: Direction,
  node: { id: string; ratio: number },
  ctx: RenderCtx,
): void {
  divider.addEventListener("mousedown", (down) => {
    down.preventDefault();
    const rect = split.getBoundingClientRect();
    const size = direction === "row" ? rect.width : rect.height;
    const start = direction === "row" ? rect.left : rect.top;
    let ratio = node.ratio;

    const onMove = (move: MouseEvent) => {
      const pos = direction === "row" ? move.clientX : move.clientY;
      ratio = clamp((pos - start) / size, 0.1, 0.9);
      applyRatio(first, second, ratio);
    };
    const onUp = async () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      try {
        const clamped = await ctx.commitRatio(node.id, ratio);
        node.ratio = clamped;
        applyRatio(first, second, clamped);
      } catch (e) {
        console.warn("[resizeSplit]", e);
        applyRatio(first, second, node.ratio); // revert to authoritative value
      }
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  });
}
