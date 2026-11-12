import type { OptimizedBuffer } from "@opentui/core";
import { mix, rgb } from "./color.ts";
import { pixels } from "./pixelfont.ts";

/**
 * A full-screen screensaver: a big title made and unmade by effects that use every cell.
 * Text cannot glide smoothly in a terminal, so nothing here does. Everything is many small
 * pieces, each moving one cell at a time, quickly, or changing colour where it stands.
 */

export interface SaverColours {
  bg: string;
  ink: string;
  glow: string;
  dim: string;
  accents: string[];
}

/** How long each effect runs, in seconds. */
export const EFFECT_SECONDS = 10;
const GLYPHS = "0123456789ABCDEF<>/\\|=+*:-.#%&$@?";

interface Cell {
  x: number;
  y: number;
}
interface Block {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Layout {
  W: number;
  H: number;
  cells: Cell[];
  blocks: Block[];
  box: Block;
  /** Which cells belong to the title, by y * W + x. */
  lit: Uint8Array;
}

/** A repeatable random number in 0..1 from a few integers. */
export function hash(a: number, b = 0, c = 0): number {
  let h = Math.imul(a | 0, 374761393) ^ Math.imul(b | 0, 668265263) ^ Math.imul(c | 0, 2246822519);
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  h ^= h >>> 16;
  return (h >>> 0) / 4294967296;
}

const clamp = (v: number, lo = 0, hi = 1) => Math.max(lo, Math.min(hi, v));
const easeInOut = (u: number) => (u < 0.5 ? 4 * u * u * u : 1 - (-2 * u + 2) ** 3 / 2);

type Effect = (s: Scene, t: number) => void;

/** One frame's drawing surface, with the title laid out and colours ready. */
class Scene {
  constructor(
    readonly buf: OptimizedBuffer,
    readonly L: Layout,
    readonly c: SaverColours,
    readonly seed: number,
  ) {}

  /** A colour between a and b, in sixteen steps, so unchanged cells stay unchanged. */
  shade(a: string, b: string, t: number): string {
    return mix(a, b, Math.round(clamp(t) * 16) / 16);
  }
  fill(x: number, y: number, color: string): void {
    if (x >= 0 && y >= 0 && x < this.L.W && y < this.L.H) this.buf.setCell(x, y, " ", rgb(color), rgb(color));
  }
  put(x: number, y: number, ch: string, color: string): void {
    if (x >= 0 && y >= 0 && x < this.L.W && y < this.L.H) this.buf.setCell(x, y, ch, rgb(color), rgb(this.c.bg));
  }
  glyph(a: number, b: number, c: number): string {
    return GLYPHS[Math.floor(hash(a, b, c + this.seed) * GLYPHS.length)]!;
  }
  r(a: number, b = 0, c = 0): number {
    return hash(a, b, c + this.seed * 7919);
  }
}

/** Falling characters; the title lights up wherever a drop passes through it. */
const rain: Effect = (s, t) => {
  const { W, H, lit, cells } = s.L;
  const drops: { x: number; v: number; st: number }[] = [];
  for (let x = 0; x < W; x++) {
    const inTitle = x >= s.L.box.x && x < s.L.box.x + s.L.box.w;
    if (!inTitle && s.r(x, 1) < 0.45) continue;
    const v = 16 + 28 * s.r(x, 2);
    const len = 6 + Math.floor(14 * s.r(x, 3));
    const st = s.r(x, 4) * 2.5;
    const period = (H + len) / v + 0.4 + s.r(x, 5) * 1.6;
    drops.push({ x, v, st });
    const phase = t - st;
    if (phase < 0) continue;
    const cycle = Math.floor(phase / period);
    if (st + cycle * period > 7.6) continue;
    const head = (phase - cycle * period) * v;
    for (let i = 0; i < len; i++) {
      const y = Math.floor(head) - i;
      if (y < 0 || y >= H || lit[y * W + x]) continue;
      const b = 1 - i / len;
      s.put(x, y, s.glyph(x, y, Math.floor(t * 9 + x)), i === 0 ? s.c.glow : s.shade(s.c.bg, s.c.ink, b * 0.9));
    }
  }
  const startOf = new Map(drops.map((d) => [d.x, d]));
  for (const { x, y } of cells) {
    const d = startOf.get(x);
    if (!d) continue;
    const at = d.st + (y + 1) / d.v;
    if (t < at) continue;
    const age = t - at;
    let col = age < 0.35 ? s.shade(s.c.glow, s.c.ink, age / 0.35) : s.c.ink;
    if (t > 8) col = s.shade(s.c.bg, col, (10 - t) / 2);
    s.fill(x, y, col);
  }
};

/** A sparse field of flickering characters; the title resolves out of scrambled letters. */
const decrypt: Effect = (s, t) => {
  const { W, H, cells } = s.L;
  for (let y = 0; y < H; y++)
    for (let x = 0; x < W; x++) {
      const n = s.r(x, y, 7);
      if (n > 0.035) continue;
      const at = (n / 0.035) * 7;
      const age = t - at;
      if (age < 0 || age > 1.6) continue;
      const b = Math.sin((age / 1.6) * Math.PI);
      s.put(x, y, s.glyph(x, y, Math.floor(t * 12)), s.shade(s.c.bg, s.c.dim, b));
    }
  const a = s.c.accents[0] ?? s.c.ink;
  for (const { x, y } of cells) {
    const on = 1.2 + (x / W) * 2.6 + s.r(x, y, 8) * 1.1;
    const off = 7.2 + s.r(x, y, 9) * 1.8;
    if (t < on - 1 || t > off + 0.6) continue;
    if (t < on || t > off) {
      s.put(x, y, s.glyph(x, y, Math.floor(t * 14)), t < on ? s.shade(s.c.dim, a, t - (on - 1)) : a);
      continue;
    }
    const age = t - on;
    s.fill(x, y, age < 0.3 ? s.shade(s.c.glow, s.c.ink, age / 0.3) : s.c.ink);
  }
};

/** Beams of light sweep across, then down, leaving the title lit; then sweep it away. */
const beams: Effect = (s, t) => {
  const { W, H, lit, box } = s.L;
  const A = s.c.accents[0] ?? s.c.ink;
  const B = s.c.accents[1] ?? s.c.glow;
  const colour = new Map<number, string>();
  // Across: every row of the title, and some others.
  for (let y = 0; y < H; y++) {
    const inTitle = y >= box.y && y < box.y + box.h;
    if (!inTitle && s.r(y, 10) > 0.3) continue;
    for (const [start, fromRight, clear] of [
      [s.r(y, 11) * 1.4, y % 2 === 1, false],
      [7.4 + s.r(y, 13) * 1.2, y % 2 === 0, true],
    ] as const) {
      const head = (t - start) * 110;
      if (head < 0) continue;
      for (let i = 0; i < 34; i++) {
        const p = Math.floor(head) - i;
        if (p < 0 || p >= W) continue;
        const x = fromRight ? W - 1 - p : p;
        if (!lit[y * W + x] || i < 2) s.fill(x, y, i < 2 ? s.c.glow : s.shade(s.c.bg, A, (1 - i / 34) * 0.8));
      }
      for (let x = box.x; x < box.x + box.w; x++) {
        if (!lit[y * W + x]) continue;
        const p = fromRight ? W - 1 - x : x;
        if (head >= p) colour.set(y * W + x, clear ? s.c.bg : A);
      }
    }
  }
  // Down: every column of the title, recolouring it as it passes.
  for (let x = box.x; x < box.x + box.w; x++) {
    const start = 4.2 + s.r(x, 12) * 1.4;
    const head = (t - start) * 40;
    if (head < 0) continue;
    for (let i = 0; i < 12; i++) {
      const y = Math.floor(head) - i;
      if (y < 0 || y >= H || lit[y * W + x]) continue;
      s.fill(x, y, i === 0 ? s.c.glow : s.shade(s.c.bg, B, (1 - i / 12) * 0.6));
    }
    for (let y = box.y; y < box.y + box.h; y++) {
      const k = y * W + x;
      if (lit[k] && head >= y && colour.get(k) === A) colour.set(k, B);
    }
  }
  for (const [k, col] of colour) if (col !== s.c.bg) s.fill(k % W, Math.floor(k / W), col);
};

/** The title's pixels fly in from all over the screen, shimmer, then burst apart. */
const swarm: Effect = (s, t) => {
  const { W, H, blocks, box } = s.L;
  const cx = box.x + box.w / 2;
  const cy = box.y + box.h / 2;
  const a = s.c.accents[2] ?? s.c.ink;
  blocks.forEach((b, i) => {
    let x: number;
    let y: number;
    let col: string;
    if (t < 7) {
      const sx = s.r(i, 21) * (W - b.w);
      const sy = s.r(i, 22) * (H - b.h);
      const u = clamp((t - s.r(i, 23) * 1.4) / 2.4);
      const e = easeInOut(u);
      const wob = Math.sin(Math.PI * u) * (s.r(i, 24) - 0.5) * 16;
      const dx = b.x - sx;
      const dy = b.y - sy;
      const len = Math.hypot(dx, dy) || 1;
      x = sx + dx * e + (-dy / len) * wob;
      y = sy + dy * e + (dx / len) * wob * 0.5;
      col = u >= 1 ? s.shade(s.c.ink, s.c.glow, 0.5 + 0.5 * Math.sin(b.x * 0.18 - t * 5)) : s.shade(s.c.dim, a, u);
      if (u <= 0) col = s.shade(s.c.bg, s.c.dim, 0.4 + 0.4 * Math.sin(t * 6 + i));
    } else {
      const tau = Math.max(0, t - 7 - s.r(i, 25) * 0.3);
      const dx = b.x + b.w / 2 - cx;
      const dy = b.y + b.h / 2 - cy;
      const len = Math.hypot(dx, dy * 2) || 1;
      const sp = 25 + 30 * s.r(i, 26);
      x = b.x + (dx / len) * sp * tau;
      y = b.y + (dy / len) * sp * 0.5 * tau + 14 * tau * tau;
      col = s.shade(s.c.bg, a, 1 - tau / 2.6);
    }
    const X = Math.round(x);
    const Y = Math.round(y);
    for (let yy = 0; yy < b.h; yy++) for (let xx = 0; xx < b.w; xx++) s.fill(X + xx, Y + yy, col);
  });
};

const EFFECTS: Record<string, Effect> = { rain, decrypt, beams, swarm };
export const EFFECT_NAMES = Object.keys(EFFECTS);

export class Saver {
  private layout: Layout | null = null;
  private key = "";
  private order: string[] = [];
  private started = 0;

  constructor(
    private text: string,
    private readonly effects: string[] = EFFECT_NAMES,
  ) {}

  /** Begin from the first effect, in a new random order. */
  start(now: number): void {
    this.started = now;
    const pool = this.effects.filter((e) => EFFECTS[e]);
    const list = pool.length ? pool : EFFECT_NAMES;
    this.order = [...list].sort(() => Math.random() - 0.5);
  }

  /** Which effect is showing and how far into it, for tests and labels. */
  at(now: number): { effect: string; t: number; round: number } {
    const secs = (now - this.started) / 1000;
    const round = Math.floor(secs / EFFECT_SECONDS);
    return { effect: this.order[round % this.order.length] ?? "rain", t: secs - round * EFFECT_SECONDS, round };
  }

  /** Draw the whole screen. `caption` (such as the time) sits small under the title. */
  draw(buf: OptimizedBuffer, now: number, c: SaverColours, caption = ""): void {
    const L = this.lay(buf.width, buf.height);
    buf.fillRect(0, 0, buf.width, buf.height, rgb(c.bg));
    const { effect, t, round } = this.at(now);
    EFFECTS[effect]!(new Scene(buf, L, c, round * 31 + 7), t);
    if (caption && L.box.y + L.box.h + 2 < L.H) {
      const x = Math.max(0, Math.floor((L.W - caption.length) / 2));
      buf.drawText(caption, x, L.box.y + L.box.h + 2, rgb(c.dim), rgb(c.bg));
    }
  }

  setText(text: string): void {
    if (text !== this.text) [this.text, this.key] = [text, ""];
  }

  /** The title as big as fits: each pixel two cells wide and one tall, times a scale. */
  private lay(W: number, H: number): Layout {
    const key = `${W}x${H}:${this.text}`;
    if (this.layout && key === this.key) return this.layout;
    const { pixels: px, cols, rows } = pixels(this.text);
    let k = 1;
    while (cols * 2 * (k + 1) <= W * 0.82 && rows * (k + 1) <= H * 0.5) k++;
    const bw = 2 * k;
    const bh = k;
    const box = { x: Math.floor((W - cols * bw) / 2), y: Math.floor((H - rows * bh) / 2) - 1, w: cols * bw, h: rows * bh };
    const lit = new Uint8Array(W * H);
    const cells: Cell[] = [];
    const blocks: Block[] = [];
    for (const p of px) {
      const b = { x: box.x + p.col * bw, y: box.y + p.row * bh, w: bw, h: bh };
      blocks.push(b);
      for (let y = b.y; y < b.y + bh; y++)
        for (let x = b.x; x < b.x + bw; x++)
          if (x >= 0 && y >= 0 && x < W && y < H) {
            lit[y * W + x] = 1;
            cells.push({ x, y });
          }
    }
    this.layout = { W, H, cells, blocks, box, lit };
    this.key = key;
    return this.layout;
  }
}
