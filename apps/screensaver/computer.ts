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
  const monitorRows = Math.max(9, Math.round(h * 0.8));
  const bezel = Math.max(2, Math.round(w / 34));
  const screen = { x: bezel, y: 2, w: w - 2 * bezel, h: monitorRows - 5 };
  return { w, h, monitorRows, screen, chinRow: screen.y + screen.h + 1, bezel };
}

/** Steps of a cell the case glides by. */
const STEPS = 8;

/**
 * How many steps a cell can be split into on this terminal. Terminals that draw block
 * characters themselves (most modern ones) fill partial blocks exactly, so a picture can sit
 * at any eighth of a cell. Apple's Terminal draws them from the font, a pixel off, leaving a
 * hairline of the wrong colour at the cell's edge, so there pictures sit on whole cells.
 */
export function blockSteps(env = process.env): number {
  if (env.PHOSPHOR_BLOCK_STEPS) return Number(env.PHOSPHOR_BLOCK_STEPS) >= 8 ? 8 : 1;
  return env.TERM_PROGRAM === "Apple_Terminal" ? 1 : 8;
}

/** Where to draw a computer whose top-left corner is at (fx, fy), a fraction of a cell. */
export interface Placement {
  /** The cell the pre-drawn picture's top-left corner goes on. */
  cellX: number;
  cellY: number;
  /** Eighths of a cell past that corner. */
  qx: number;
  qy: number;
  /** The whole cell the screen's contents line up with: text can only sit on whole cells. */
  snapX: number;
  snapY: number;
}

export function placement(fx: number, fy: number, steps = STEPS): Placement {
  const k = STEPS / steps;
  const ax = Math.round((fx * STEPS) / k) * k;
  const ay = Math.round((fy * STEPS) / k) * k;
  const cellX = Math.floor(ax / STEPS);
  const cellY = Math.floor(ay / STEPS);
  const qx = ax - cellX * STEPS;
  const qy = ay - cellY * STEPS;
  return { cellX, cellY, qx, qy, snapX: cellX + (qx >= STEPS / 2 ? 1 : 0), snapY: cellY + (qy >= STEPS / 2 ? 1 : 0) };
}

/**
 * The computer's case, drawn once for each of the 64 ways it can sit across a cell and
 * kept, so moving it costs one copy a frame. The outline and the base glide by eighths of a
 * cell; the screen and its dark rim stay on whole cells with the text inside them.
 */
export class Art {
  private cache = new Map<string, OptimizedBuffer>();
  private key = "";

  constructor(
    private readonly renderer: CliRenderer,
    private readonly steps = STEPS,
  ) {}

  /** The picture for this size, colour and fraction of a cell. */
  picture(g: Geometry, p: Palette, qx: number, qy: number): OptimizedBuffer {
    const key = `${g.w}x${g.h}:${p.case}:${p.screen}`;
    if (key !== this.key) this.clear(key);
    const k = `${qx},${qy}`;
    let buf = this.cache.get(k);
    if (!buf) {
      buf = OptimizedBuffer.create(g.w + 1, g.h + 1, this.renderer.widthMethod, { owner: this.renderer.nativeScene });
      let list = shapes(g, p, qx / STEPS, qy / STEPS, this.steps === 1);
      // On whole cells only, every edge lands on a cell boundary and thin details round away.
      if (this.steps === 1)
        list = list
          .map((sh) => {
            const x = Math.round(sh.x);
            const y = Math.round(sh.y);
            return { ...sh, x, y, w: Math.round(sh.x + sh.w) - x, h: Math.round(sh.y + sh.h) - y };
          })
          .filter((sh) => sh.w > 0 && sh.h > 0);
      paintShapes(buf, list, p.room);
      this.cache.set(k, buf);
    }
    return buf;
  }

  /** Draw any pictures not drawn yet, a few at a time, so no frame waits for one later. */
  warm(g: Geometry, p: Palette, budget = 2): void {
    const key = `${g.w}x${g.h}:${p.case}:${p.screen}`;
    if (key !== this.key) this.clear(key);
    for (let qy = 0; qy < STEPS && budget > 0; qy++)
      for (let qx = 0; qx < STEPS && budget > 0; qx++)
        if (!this.cache.has(`${qx},${qy}`) && qx % (STEPS / this.steps) === 0 && qy % (STEPS / this.steps) === 0) {
          this.picture(g, p, qx, qy);
          budget--;
        }
  }

  clear(key = ""): void {
    for (const b of this.cache.values()) b.destroy();
    this.cache.clear();
    this.key = key;
  }
}

/** The case as rectangles, with its top-left at (ox, oy) inside the picture. */
function shapes(g: Geometry, p: Palette, ox: number, oy: number, whole = false): Shape[] {
  const { w, h, monitorRows: mr, screen: s } = g;
  // The screen and its rim sit on whole cells, with the text.
  const sx = ox >= 0.5 ? 1 : 0;
  const sy = oy >= 0.5 ? 1 : 0;
  const at = (x: number, y: number, ww: number, hh: number, color: string): Shape => ({ x: ox + x, y: oy + y, w: ww, h: hh, color });
  const fixed = (x: number, y: number, ww: number, hh: number, color: string): Shape => ({ x: sx + x, y: sy + y, w: ww, h: hh, color });
  const out: Shape[] = [
    at(0, 0, w, mr, p.case),
    at(1, 0, w - 2, 0.25, p.caseLight),
    at(0, mr - 0.25, w, 0.25, p.caseShade),
    // The rim is half a cell deep above and below the screen; on whole cells, only at the sides.
    whole ? fixed(s.x - 1, s.y, s.w + 2, s.h, p.caseDark) : fixed(s.x - 1, s.y - 0.5, s.w + 2, s.h + 1, p.caseDark),
    fixed(s.x, s.y, s.w, s.h, p.screen),
  ];
  const standW = Math.max(4, Math.round(w / 5));
  out.push(at(Math.round(w / 2 - standW / 2), mr, standW, 1, p.caseDark));
  const bh = h - mr - 1;
  if (bh >= 2) {
    const bx = Math.round(w / 20);
    const bw = w - 2 * bx;
    out.push(at(bx, mr + 1, bw, bh, p.case));
    out.push(at(bx + 1, mr + 1, bw - 2, 0.25, p.caseLight));
    out.push(at(bx, mr + 1 + bh - 0.25, bw, 0.25, p.caseShade));
    const vents = Math.min(6, Math.max(3, Math.round(bw / 30)));
    for (let i = 0; i < vents; i++) out.push(at(bx + 3 + i * 2, mr + 1.75, 1, bh - 1.5, p.caseDark));
    const slot = Math.max(4, Math.round(w / 7));
    const sy2 = mr + 1 + bh / 2 - 0.25;
    out.push(at(Math.round(w * 0.55), sy2, slot, 0.5, p.caseDark));
    out.push(at(Math.round(w * 0.74), sy2, slot, 0.5, p.caseDark));
  }
  return out;
}

/** The label and power light on the chin, which sit on whole cells like the screen. */
export function drawChin(buf: OptimizedBuffer, snapX: number, snapY: number, g: Geometry, p: Palette, label: string, ledOn: boolean): void {
  const chin = snapY + g.chinRow;
  const room = g.w - 2 * g.bezel - 4;
  if (room > 0) buf.drawText(label.slice(0, room), snapX + g.bezel + 1, chin, rgb(p.caseDark), rgb(p.case), TextAttributes.BOLD);
  buf.setCell(snapX + g.w - g.bezel - 2, chin, "▬", rgb(ledOn ? "#5dff86" : p.caseDark), rgb(p.case));
}
