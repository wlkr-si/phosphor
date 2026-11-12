import type { OptimizedBuffer } from "@opentui/core";
import { rgb } from "./color.ts";

/** A filled rectangle, in whole cells. */
export interface Shape {
  x: number;
  y: number;
  w: number;
  h: number;
  color: string;
}

/** Paint rectangles into `buf` as coloured cells, later ones on top, over a background. */
export function paintShapes(buf: OptimizedBuffer, shapes: Shape[], background: string): void {
  buf.fillRect(0, 0, buf.width, buf.height, rgb(background));
  for (const s of shapes) buf.fillRect(s.x, s.y, s.w, s.h, rgb(s.color));
}
