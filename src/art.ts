import { readFileSync } from "node:fs";
import type { OptimizedBuffer } from "@opentui/core";
import { rgb } from "./color.ts";
import type { Pixel } from "./pixelfont.ts";

/**
 * A picture drawn in a text file with block characters (█ ▀ ▄), one terminal cell per
 * character, such as a logo. Each cell holds two square pixels, one above the other.
 */
export interface Art {
  lines: string[];
  w: number;
  h: number;
}

/** Read a picture from text, dropping blank lines around it. */
export function parseArt(text: string): Art {
  const lines = text.replace(/\r/g, "").split("\n").map((l) => l.replace(/\s+$/, ""));
  while (lines.length && !lines[lines.length - 1]) lines.pop();
  while (lines.length && !lines[0]) lines.shift();
  return { lines, w: Math.max(0, ...lines.map((l) => [...l].length)), h: lines.length };
}

export function loadArt(path: string): Art {
  let text: string;
  try {
    text = readFileSync(path, "utf8");
  } catch (e) {
    throw new Error(`could not read ${path}: ${(e as Error).message}`);
  }
  const art = parseArt(text);
  if (!art.h) throw new Error(`${path} has no picture in it`);
  return art;
}

/** The picture's lit pixels: ▀ lights the top half of a cell, ▄ the bottom, anything else both. */
export function artPixels(art: Art): { pixels: Pixel[]; cols: number; rows: number } {
  const out: Pixel[] = [];
  art.lines.forEach((line, y) =>
    [...line].forEach((ch, x) => {
      if (ch === " ") return;
      if (ch !== "▄") out.push({ col: x, row: 2 * y });
      if (ch !== "▀") out.push({ col: x, row: 2 * y + 1 });
    }),
  );
  return { pixels: out, cols: art.w, rows: art.h * 2 };
}

/**
 * Draw the picture in one colour, always as the cell's background so every part of it is the
 * same shade: full blocks become coloured spaces, and half blocks draw the empty half instead.
 * Terminals draw block characters slightly differently (Apple's leaves thin gaps, Ghostty
 * brightens them), so this keeps the ink solid and any difference at the picture's edge.
 */
export function drawArt(buf: OptimizedBuffer, art: Art, x: number, y: number, color: string, bg: string): void {
  const fg = rgb(color);
  const back = rgb(bg);
  art.lines.forEach((line, r) =>
    [...line].forEach((ch, k) => {
      if (ch === " ") return;
      if (ch === "█") buf.setCell(x + k, y + r, " ", fg, fg);
      else if (ch === "▀") buf.setCell(x + k, y + r, "▄", back, fg);
      else if (ch === "▄") buf.setCell(x + k, y + r, "▀", back, fg);
      else buf.setCell(x + k, y + r, ch, fg, back);
    }),
  );
}
