import { OptimizedBuffer, TextAttributes, type CliRenderer } from "@opentui/core";
import { rgb } from "../../src/color.ts";
import { paintShapes, type Shape } from "../../src/shapes.ts";
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
  const monitorRows = Math.max(9, Math.round(h * 0.82));
  const bezel = Math.max(2, Math.round(w / 40));
  const screen = { x: bezel, y: 1, w: w - 2 * bezel, h: monitorRows - 3 };
  return { w, h, monitorRows, screen, chinRow: screen.y + screen.h + 1, bezel };
}

/** The case as rectangles on whole cells: a monitor, a stand, and a desktop box with vents. */
function shapes(g: Geometry, p: Palette): Shape[] {
  const { w, h, monitorRows: mr, screen: s } = g;
  const out: Shape[] = [
    { x: 0, y: 0, w, h: mr, color: p.case },
    { x: s.x - 1, y: s.y, w: s.w + 2, h: s.h, color: p.caseDark },
    { x: s.x, y: s.y, w: s.w, h: s.h, color: p.screen },
  ];
  const standW = Math.max(4, Math.round(w / 5));
  out.push({ x: Math.round(w / 2 - standW / 2), y: mr, w: standW, h: 1, color: p.caseDark });
  const bh = h - mr - 1;
  if (bh >= 2) {
    const bx = Math.round(w / 20);
    const bw = w - 2 * bx;
    out.push({ x: bx, y: mr + 1, w: bw, h: bh, color: p.case });
    const vents = Math.min(6, Math.max(3, Math.round(bw / 30)));
    if (bh >= 3) for (let i = 0; i < vents; i++) out.push({ x: bx + 3 + i * 2, y: mr + 2, w: 1, h: bh - 2, color: p.caseDark });
    const slot = Math.max(4, Math.round(w / 7));
    const sy = mr + 1 + Math.floor(bh / 2);
    out.push({ x: Math.round(w * 0.55), y: sy, w: slot, h: 1, color: p.caseDark });
    out.push({ x: Math.round(w * 0.74), y: sy, w: slot, h: 1, color: p.caseDark });
  }
  return out;
}

/** The case, drawn once per size and colour and kept. */
export class Art {
  private buf: OptimizedBuffer | null = null;
  private key = "";

  constructor(private readonly renderer: CliRenderer) {}

  picture(g: Geometry, p: Palette): OptimizedBuffer {
    const key = `${g.w}x${g.h}:${p.case}:${p.screen}`;
    if (this.buf && key === this.key) return this.buf;
    this.buf?.destroy();
    this.buf = OptimizedBuffer.create(g.w, g.h, this.renderer.widthMethod, { owner: this.renderer.nativeScene });
    paintShapes(this.buf, shapes(g, p), p.room);
    this.key = key;
    return this.buf;
  }

  clear(): void {
    this.buf?.destroy();
    this.buf = null;
    this.key = "";
  }
}

/** The label and power light on the chin. */
export function drawChin(buf: OptimizedBuffer, x: number, y: number, g: Geometry, p: Palette, label: string, ledOn: boolean): void {
  const chin = y + g.chinRow;
  const room = g.w - 2 * g.bezel - 4;
  if (room > 0) buf.drawText(label.slice(0, room), x + g.bezel + 1, chin, rgb(p.caseDark), rgb(p.case), TextAttributes.BOLD);
  buf.setCell(x + g.w - g.bezel - 2, chin, "▬", rgb(ledOn ? "#5dff86" : p.caseDark), rgb(p.case));
}
