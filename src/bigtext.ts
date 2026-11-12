import { fonts, type OptimizedBuffer } from "@opentui/core";
import { rgb } from "./color.ts";

const font = fonts.tiny;

/** Plainer digits than the font's own, so a clock reads at a glance. */
const DIGITS: Record<string, string[]> = {
  "0": ["█▀█", "█▄█"],
  "1": ["▄█ ", "▄█▄"],
  "2": ["▀▀█", "█▄▄"],
  "3": ["▀██", "▄▄█"],
  "4": ["█ █", "▀▀█"],
  "5": ["█▀▀", "▄▄█"],
  "6": ["█▄▄", "█▄█"],
  "7": ["▀▀█", "  █"],
  "8": ["███", "█▄█"],
  "9": ["█▀█", "▀▀█"],
  ":": ["▄", "▄"],
};

const glyph = (ch: string): string[] =>
  DIGITS[ch] ?? (font.chars as Record<string, string[]>)[ch.toUpperCase()] ?? font.chars[" "];

/** How many cells wide `text` is in big letters. */
export function bigWidth(text: string): number {
  return [...text].reduce((w, ch, i) => w + [...glyph(ch)[0]!].length + (i ? font.letterspace_size : 0), 0);
}

/**
 * Write `text` in big letters two rows tall. Full blocks become coloured spaces: some
 * terminals (Apple's among them) draw the full-block character smaller than the cell.
 */
export function bigText(buf: OptimizedBuffer, text: string, x: number, y: number, color: string, bg: string): number {
  const fg = rgb(color);
  const back = rgb(bg);
  let cx = x;
  for (const [i, ch] of [...text].entries()) {
    if (i) cx += font.letterspace_size;
    const rows = glyph(ch);
    const w = [...rows[0]!].length;
    rows.forEach((row, r) =>
      [...row].forEach((c, k) => {
        if (c === "█") buf.setCell(cx + k, y + r, " ", fg, fg);
        else if (c !== " ") buf.setCell(cx + k, y + r, c, fg, back);
      }),
    );
    cx += w;
  }
  return cx - x;
}
