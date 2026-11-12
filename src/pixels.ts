import type { OptimizedBuffer } from "@opentui/core";
import { rgb } from "./color.ts";

/**
 * A small picture made of dots, two dots to a terminal cell (top and bottom half).
 * Dots are roughly square, so shapes keep their proportions. Unset dots are see-through.
 */
export class Pixels {
  readonly dots: (string | null)[];

  constructor(
    readonly w: number,
    readonly h: number,
  ) {
    this.dots = new Array(w * h).fill(null);
  }

  set(x: number, y: number, c: string | null): void {
    if (x >= 0 && y >= 0 && x < this.w && y < this.h) this.dots[y * this.w + x] = c;
  }

  get(x: number, y: number): string | null {
    if (x < 0 || y < 0 || x >= this.w || y >= this.h) return null;
    return this.dots[y * this.w + x] ?? null;
  }

  rect(x: number, y: number, w: number, h: number, c: string | null): void {
    for (let yy = Math.max(0, y); yy < Math.min(this.h, y + h); yy++)
      for (let xx = Math.max(0, x); xx < Math.min(this.w, x + w); xx++) this.dots[yy * this.w + xx] = c;
  }

  /** Paint the dots onto `buf` with the top-left dot at cell (cx, cy). */
  draw(buf: OptimizedBuffer, cx: number, cy: number, behind: string): void {
    const rows = Math.ceil(this.h / 2);
    for (let r = 0; r < rows; r++) {
      const y = cy + r;
      if (y < 0 || y >= buf.height) continue;
      for (let x = 0; x < this.w; x++) {
        const bx = cx + x;
        if (bx < 0 || bx >= buf.width) continue;
        const top = this.get(x, r * 2);
        const bottom = this.get(x, r * 2 + 1);
        if (top === null && bottom === null) continue;
        if (top === bottom) buf.setCell(bx, y, " ", rgb(top!), rgb(top!));
        else buf.setCell(bx, y, "▀", rgb(top ?? behind), rgb(bottom ?? behind));
      }
    }
  }
}
