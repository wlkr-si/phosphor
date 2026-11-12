import { RGBA } from "@opentui/core";

const cache = new Map<string, RGBA>();

/** A colour from "#rrggbb", cached so drawing never allocates. */
export function rgb(hex: string): RGBA {
  let c = cache.get(hex);
  if (!c) {
    c = RGBA.fromHex(hex);
    cache.set(hex, c);
  }
  return c;
}

/** Mix two "#rrggbb" colours: t = 0 gives a, t = 1 gives b. */
export function mix(a: string, b: string, t: number): string {
  const p = (h: string, i: number) => parseInt(h.slice(1 + i * 2, 3 + i * 2), 16);
  const out = [0, 1, 2].map((i) => Math.round(p(a, i) + (p(b, i) - p(a, i)) * t));
  return "#" + out.map((v) => v.toString(16).padStart(2, "0")).join("");
}
