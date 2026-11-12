import { TextAttributes, type OptimizedBuffer } from "@opentui/core";
import { Pixels } from "../../src/pixels.ts";
import { rgb } from "../../src/color.ts";
import type { Palette } from "./theme.ts";

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Where the parts of a computer `w` x `h` cells sit, relative to its top-left cell. */
export interface Geometry {
  w: number;
  h: number;
  monitorRows: number;
  screen: Rect;
  chinRow: number;
  bezel: number;
}

export function geometry(w: number, h: number): Geometry {
  const monitorRows = Math.max(9, Math.round(h * 0.8));
  const bezel = Math.max(2, Math.round(w / 34));
  const screen = { x: bezel, y: 2, w: w - 2 * bezel, h: monitorRows - 5 };
  return { w, h, monitorRows, screen, chinRow: screen.y + screen.h + 1, bezel };
}

/** The case is drawn once per size and colour; only the light changes between frames. */
let cached: { key: string; px: Pixels } | null = null;

function casePixels(g: Geometry, p: Palette): Pixels {
  const key = `${g.w}x${g.h}:${p.case}`;
  if (cached?.key === key) return cached.px;
  const { w, h, monitorRows: mr, screen: s } = g;
  const px = new Pixels(w, h * 2);
  const md = mr * 2; // the monitor's height in dots

  // The monitor: a rounded box with a lit top edge and a shaded bottom edge.
  px.rect(0, 0, w, md, p.case);
  px.rect(1, 0, w - 2, 1, p.caseLight);
  px.rect(0, md - 1, w, 1, p.caseShade);
  for (const [x, y] of [[0, 0], [w - 1, 0], [0, md - 1], [w - 1, md - 1]] as const) px.set(x, y, null);

  // The screen sits in a dark rim half a cell deep.
  px.rect(s.x - 1, s.y * 2 - 1, s.w + 2, s.h * 2 + 2, p.caseDark);
  px.rect(s.x, s.y * 2, s.w, s.h * 2, p.screen);

  // The stand and the desktop case underneath, with vents and two drive slots.
  const sw = Math.max(4, Math.round(w / 5));
  px.rect(Math.round(w / 2 - sw / 2), md, sw, 2, p.caseDark);
  const top = md + 2;
  const bh = h * 2 - top;
  if (bh >= 4) {
    const bx = Math.round(w / 20);
    const bw = w - 2 * bx;
    px.rect(bx, top, bw, bh, p.case);
    px.rect(bx + 1, top, bw - 2, 1, p.caseLight);
    px.rect(bx, top + bh - 1, bw, 1, p.caseShade);
    px.set(bx, top, null);
    px.set(bx + bw - 1, top, null);
    px.set(bx, top + bh - 1, null);
    px.set(bx + bw - 1, top + bh - 1, null);
    const vents = Math.min(6, Math.max(3, Math.round(bw / 30)));
    for (let i = 0; i < vents; i++) px.rect(bx + 3 + i * 2, top + 2, 1, bh - 4, p.caseDark);
    const slot = Math.max(4, Math.round(w / 7));
    const sy = top + Math.floor(bh / 2) - 1;
    px.rect(Math.round(w * 0.55), sy, slot, 1, p.caseDark);
    px.rect(Math.round(w * 0.74), sy, slot, 1, p.caseDark);
  }
  cached = { key, px };
  return px;
}

/** Draw the computer at cell (x, y). The screen is left blank for the board. */
export function drawComputer(
  buf: OptimizedBuffer,
  x: number,
  y: number,
  g: Geometry,
  p: Palette,
  label: string,
  ledOn: boolean,
): void {
  casePixels(g, p).draw(buf, x, y, p.room);
  const chin = y + g.chinRow;
  const room = g.w - 2 * g.bezel - 4;
  if (room > 0) buf.drawText(label.slice(0, room), x + g.bezel + 1, chin, rgb(p.caseDark), rgb(p.case), TextAttributes.BOLD);
  buf.setCell(x + g.w - g.bezel - 2, chin, "▬", rgb(ledOn ? "#5dff86" : p.caseDark), rgb(p.case));
}
