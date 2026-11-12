import type { OptimizedBuffer } from "@opentui/core";
import { rgb } from "./color.ts";

/** A filled rectangle in cells; edges may fall anywhere inside a cell. */
export interface Shape {
  x: number;
  y: number;
  w: number;
  h: number;
  color: string;
}

const LEFT = ["▏", "▎", "▍", "▌", "▋", "▊", "▉"];
const LOWER = ["▁", "▂", "▃", "▄", "▅", "▆", "▇"];
const N = 8;

/**
 * Paint rectangles into `buf`, later ones on top, with edges placed to an eighth of a cell.
 * A cell crossed by an edge becomes a partial block: left eighths for an upright edge,
 * lower eighths for a level one, whichever matches the shapes better. That is how a
 * picture moves smoothly across a grid of characters.
 */
export function paintShapes(buf: OptimizedBuffer, shapes: Shape[], background: string): void {
  const palette = [background, ...new Set(shapes.map((s) => s.color))];
  const index = new Map(palette.map((c, i) => [c, i]));
  const grid = new Uint8Array(N * N);
  const counts = new Uint16Array(palette.length);

  for (let r = 0; r < buf.height; r++) {
    for (let c = 0; c < buf.width; c++) {
      // Which shapes touch this cell, and is the top one that fills it the last word?
      let solid = 0;
      let partial = false;
      for (const s of shapes) {
        if (s.x >= c + 1 || s.y >= r + 1 || s.x + s.w <= c || s.y + s.h <= r) continue;
        if (s.x <= c && s.y <= r && s.x + s.w >= c + 1 && s.y + s.h >= r + 1) {
          solid = index.get(s.color)!;
          partial = false;
        } else partial = true;
      }
      if (!partial) {
        const col = rgb(palette[solid]!);
        buf.setCell(c, r, " ", col, col);
        continue;
      }
      // Sample the cell at 8 x 8 points.
      grid.fill(0);
      for (const s of shapes) {
        const i0 = Math.max(0, Math.round((s.x - c) * N));
        const i1 = Math.min(N, Math.round((s.x + s.w - c) * N));
        const j0 = Math.max(0, Math.round((s.y - r) * N));
        const j1 = Math.min(N, Math.round((s.y + s.h - r) * N));
        if (i0 >= i1 || j0 >= j1) continue;
        const k = index.get(s.color)!;
        for (let j = j0; j < j1; j++) grid.fill(k, j * N + i0, j * N + i1);
      }
      const [ch, fg, bg] = fit(grid, counts);
      buf.setCell(c, r, ch, rgb(palette[fg]!), rgb(palette[bg]!));
    }
  }
}

/** The single character that best shows an 8 x 8 sample: [char, ink, paper]. */
function fit(grid: Uint8Array, counts: Uint16Array): [string, number, number] {
  const most = (pick: (i: number, j: number) => boolean): [number, number] => {
    counts.fill(0);
    let n = 0;
    for (let j = 0; j < N; j++)
      for (let i = 0; i < N; i++)
        if (pick(i, j)) {
          counts[grid[j * N + i]!]!++;
          n++;
        }
    let best = 0;
    for (let k = 1; k < counts.length; k++) if (counts[k]! > counts[best]!) best = k;
    return [best, n - counts[best]!];
  };
  const [all, allErr] = most(() => true);
  let best: [string, number, number] = [" ", all, all];
  let err = allErr;
  if (err === 0) return best;
  for (let k = 1; k < N; k++) {
    const [b, eb] = most((_, j) => j >= N - k);
    const [t, et] = most((_, j) => j < N - k);
    if (eb + et < err) [best, err] = [[LOWER[k - 1]!, b, t], eb + et];
    const [l, el] = most((i) => i < k);
    const [rr, er] = most((i) => i >= k);
    if (el + er < err) [best, err] = [[LEFT[k - 1]!, l, rr], el + er];
  }
  return best;
}
