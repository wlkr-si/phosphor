import type { OptimizedBuffer } from "@opentui/core";
import { rgb } from "./color.ts";

/**
 * Big letters five rows tall made only of whole coloured cells, each pixel two cells wide and
 * one tall. No block characters, so every terminal draws them the same, with no seams.
 */
const GLYPHS: Record<string, string[]> = {
  "0": ["###", "#.#", "#.#", "#.#", "###"],
  "1": [".#.", "##.", ".#.", ".#.", "###"],
  "2": ["###", "..#", "###", "#..", "###"],
  "3": ["###", "..#", ".##", "..#", "###"],
  "4": ["#.#", "#.#", "###", "..#", "..#"],
  "5": ["###", "#..", "###", "..#", "###"],
  "6": ["###", "#..", "###", "#.#", "###"],
  "7": ["###", "..#", "..#", "..#", "..#"],
  "8": ["###", "#.#", "###", "#.#", "###"],
  "9": ["###", "#.#", "###", "..#", "###"],
  A: ["###", "#.#", "###", "#.#", "#.#"],
  B: ["##.", "#.#", "##.", "#.#", "##."],
  C: ["###", "#..", "#..", "#..", "###"],
  D: ["##.", "#.#", "#.#", "#.#", "##."],
  E: ["###", "#..", "##.", "#..", "###"],
  F: ["###", "#..", "##.", "#..", "#.."],
  G: ["###", "#..", "#.#", "#.#", "###"],
  H: ["#.#", "#.#", "###", "#.#", "#.#"],
  I: ["###", ".#.", ".#.", ".#.", "###"],
  J: ["..#", "..#", "..#", "#.#", "###"],
  K: ["#.#", "#.#", "##.", "#.#", "#.#"],
  L: ["#..", "#..", "#..", "#..", "###"],
  M: ["#...#", "##.##", "#.#.#", "#...#", "#...#"],
  N: ["#..#", "##.#", "#.##", "#..#", "#..#"],
  O: ["###", "#.#", "#.#", "#.#", "###"],
  P: ["###", "#.#", "###", "#..", "#.."],
  Q: ["###", "#.#", "#.#", "###", "..#"],
  R: ["###", "#.#", "##.", "#.#", "#.#"],
  S: ["###", "#..", "###", "..#", "###"],
  T: ["###", ".#.", ".#.", ".#.", ".#."],
  U: ["#.#", "#.#", "#.#", "#.#", "###"],
  V: ["#.#", "#.#", "#.#", "#.#", ".#."],
  W: ["#...#", "#...#", "#.#.#", "##.##", "#...#"],
  X: ["#.#", "#.#", ".#.", "#.#", "#.#"],
  Y: ["#.#", "#.#", ".#.", ".#.", ".#."],
  Z: ["###", "..#", ".#.", "#..", "###"],
  ":": [".", "#", ".", "#", "."],
  ".": [".", ".", ".", ".", "#"],
  "-": ["...", "...", "###", "...", "..."],
  "!": ["#", "#", "#", ".", "#"],
  "/": ["..#", "..#", ".#.", "#..", "#.."],
  " ": ["..", "..", "..", "..", ".."],
};

export const BLOCK_ROWS = 5;
const PIXEL = 2;
const glyph = (ch: string) => GLYPHS[ch.toUpperCase()] ?? GLYPHS[" "]!;

/** How many cells wide `text` is in block letters. */
export function blockWidth(text: string): number {
  return [...text].reduce((w, ch, i) => w + (glyph(ch)[0]!.length + (i ? 1 : 0)) * PIXEL, 0);
}

/** Write `text` in block letters with its top-left cell at (x, y). */
export function blockText(buf: OptimizedBuffer, text: string, x: number, y: number, color: string): void {
  const ink = rgb(color);
  let cx = x;
  for (const [i, ch] of [...text].entries()) {
    if (i) cx += PIXEL;
    const g = glyph(ch);
    g.forEach((row, r) => [...row].forEach((c, k) => c === "#" && buf.fillRect(cx + k * PIXEL, y + r, PIXEL, 1, ink)));
    cx += g[0]!.length * PIXEL;
  }
}
