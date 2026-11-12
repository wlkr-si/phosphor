import { TextAttributes, type OptimizedBuffer } from "@opentui/core";
import { rgb } from "../../src/color.ts";
import type { Rect } from "./computer.ts";
import type { Palette } from "./theme.ts";
import type { CardSettings } from "./settings.ts";
import type { Reading } from "./sources.ts";
import type {
  Amount,
  ChecksData,
  DotsData,
  Fact,
  ListData,
  MachineData,
  ProgressData,
  TextData,
  Tone,
} from "./data.ts";

const BOLD = TextAttributes.BOLD;

/** A rectangle of the screen to write into, in its own coordinates, clipped to its edges. */
export class Region {
  constructor(
    readonly buf: OptimizedBuffer,
    readonly r: Rect,
    readonly p: Palette,
  ) {}

  get w(): number {
    return this.r.w;
  }
  get h(): number {
    return this.r.h;
  }

  /** Write text at (col, row); returns how many cells it took. */
  text(col: number, row: number, s: string, color: string, attrs = 0, bg = this.p.screen): number {
    if (row < 0 || row >= this.r.h || col >= this.r.w || !s) return 0;
    const chars = [...s];
    const start = Math.max(0, -col);
    const shown = chars.slice(start, start + this.r.w - Math.max(col, 0)).join("");
    if (!shown) return 0;
    this.buf.drawText(shown, this.r.x + Math.max(col, 0), this.r.y + row, rgb(color), rgb(bg), attrs);
    return [...shown].length;
  }

  /** A line of facts, three spaces apart. */
  facts(col: number, row: number, facts: Fact[]): number {
    let c = col;
    for (const [i, f] of facts.entries()) {
      if (i > 0) c += 3;
      c += this.text(c, row, f.text, this.p.tone(f.tone ?? "plain"));
    }
    return c - col;
  }

  /** A bar `w` cells wide, filled to `pct` percent. Bars sit low in their cells so stacked bars stay apart. */
  bar(col: number, row: number, w: number, pct: number, color: string): void {
    if (w <= 0 || row < 0 || row >= this.r.h) return;
    const full = Math.round((Math.max(0, Math.min(100, pct)) / 100) * w);
    for (let i = 0; i < w && col + i < this.r.w; i++)
      this.buf.setCell(this.r.x + col + i, this.r.y + row, "▆", rgb(i < full ? color : this.p.track), rgb(this.p.screen));
  }
}

function clip(s: string, n: number): string {
  if (n <= 0) return "";
  const chars = [...s];
  return chars.length <= n ? s : chars.slice(0, Math.max(0, n - 1)).join("") + "…";
}

/** Green below 70%, amber below 90%, red above. */
function level(p: Palette, pct: number): string {
  return p.tone(pct < 70 ? "good" : pct < 90 ? "warn" : "bad");
}

function amount(a: Amount): string {
  const n = (v: number) => (v >= 100 || Number.isInteger(v) ? Math.round(v).toString() : v.toFixed(1));
  return `${n(a.used)}/${n(a.total)} ${a.unit ?? "GB"}`;
}

/** Draw a card: a rounded border in its colour with its title, and its data inside. */
export function drawCard(buf: OptimizedBuffer, r: Rect, card: CardSettings, reading: Reading, p: Palette): void {
  if (r.w < 6 || r.h < 3) return;
  const accent = p.accent(card.color);
  const a = rgb(accent);
  const bg = rgb(p.screen);
  for (let x = r.x + 1; x < r.x + r.w - 1; x++) {
    buf.setCell(x, r.y, "─", a, bg);
    buf.setCell(x, r.y + r.h - 1, "─", a, bg);
  }
  for (let y = r.y + 1; y < r.y + r.h - 1; y++) {
    buf.setCell(r.x, y, "│", a, bg);
    buf.setCell(r.x + r.w - 1, y, "│", a, bg);
  }
  buf.setCell(r.x, r.y, "╭", a, bg);
  buf.setCell(r.x + r.w - 1, r.y, "╮", a, bg);
  buf.setCell(r.x, r.y + r.h - 1, "╰", a, bg);
  buf.setCell(r.x + r.w - 1, r.y + r.h - 1, "╯", a, bg);
  const head = new Region(buf, { x: r.x + 2, y: r.y, w: r.w - 4, h: 1 }, p);
  let c = head.text(0, 0, ` ${clip(card.title, r.w - 8)} `, accent, BOLD);
  if (reading.data && reading.error) head.text(c, 0, " stale ", p.tone("warn"));

  const inner = new Region(buf, { x: r.x + 2, y: r.y + 1, w: r.w - 4, h: r.h - 2 }, p);
  if (!reading.data) {
    if (reading.error) inner.text(0, 0, clip(`Can't read: ${reading.error}`, inner.w), p.tone("bad"));
    else inner.text(0, 0, "Waiting for data…", p.dim);
    return;
  }
  try {
    DRAW[card.type](inner, reading.data as never, accent);
  } catch {
    inner.text(0, 0, `This isn't ${card.type} data.`, p.tone("bad"));
  }
}

const DRAW: Record<CardSettings["type"], (g: Region, d: never, accent: string) => void> = {
  machine: machine as never,
  list: list as never,
  progress: progress as never,
  checks: checks as never,
  dots: dots as never,
  text: text as never,
};

function machine(g: Region, m: MachineData, accent: string): void {
  const p = g.p;
  if (m.unreachable) {
    g.text(0, 0, "UNREACHABLE", p.tone("bad"), BOLD);
    return;
  }
  const valueW = 13;
  const bw = Math.max(4, g.w - 8 - valueW);
  let row = 0;
  const line = (label: string, pct: number, value: string) => {
    g.text(0, row, label, p.dim);
    g.bar(8, row, bw, pct, level(p, pct));
    g.text(8 + bw + 1, row, value, p.tone("plain"), BOLD);
    row++;
  };
  if (typeof m.cpu === "number") line("CPU", m.cpu, `${Math.round(m.cpu)}%`.padStart(4));
  if (m.memory) line("Memory", (m.memory.used / Math.max(m.memory.total, 1e-9)) * 100, amount(m.memory));
  if (m.disk) line("Disk", (m.disk.used / Math.max(m.disk.total, 1e-9)) * 100, amount(m.disk));
  if (m.facts?.length) g.facts(0, row++, m.facts);

  // The processor chart fills the rest, newest on the right.
  const top = row + 1;
  const rows = g.h - top;
  const hist = m.history ?? [];
  if (rows < 2 || g.w < 10) return;
  g.text(0, top, "100", p.faint);
  g.text(0, top + rows - 1, "  0", p.faint);
  const cw = g.w - 4;
  const blocks = " ▁▂▃▄▅▆▇█";
  for (let i = 0; i < cw; i++) {
    const v = hist[hist.length - cw + i];
    for (let rr = 0; rr < rows; rr++) {
      const x = g.r.x + 4 + i;
      const y = g.r.y + top + rows - 1 - rr;
      if (v === undefined) {
        g.buf.setCell(x, y, rr === 0 ? "▁" : " ", rgb(p.faint), rgb(p.screen));
        continue;
      }
      const eighths = Math.max(0, Math.min(8, Math.round((Math.max(0, Math.min(100, v)) / 100) * rows * 8 - rr * 8)));
      const ch = rr === 0 && eighths === 0 ? "▁" : blocks[eighths]!;
      g.buf.setCell(x, y, ch, rgb(eighths === 0 ? p.faint : accent), rgb(p.screen));
    }
  }
}

function list(g: Region, d: ListData): void {
  const p = g.p;
  const items = d.items ?? [];
  if (!items.length) g.text(0, 0, "none yet", p.dim);
  const tagW = Math.max(0, ...items.map((i) => (i.tag ? [...i.tag].length + 2 : 2)));
  const rightW = Math.min(14, Math.max(0, ...items.map((i) => (i.right ? [...i.right].length + 1 : 0))));
  const hasDetail = items.some((i) => i.detail);
  const labelW = hasDetail ? Math.max(8, Math.floor((g.w - tagW - rightW) * 0.6)) : g.w - tagW - rightW;
  for (const [row, it] of items.slice(0, g.h).entries()) {
    const tone = p.tone(it.tone ?? "good");
    if (it.tag) g.text(0, row, it.tag, tone, it.tone === "bad" || it.tone === "busy" ? BOLD : 0);
    else g.text(0, row, "█", tone);
    g.text(tagW, row, clip(it.label, labelW - 1), p.tone("plain"));
    if (it.detail) g.text(tagW + labelW, row, clip(it.detail, g.w - tagW - labelW - rightW - 1), p.dim);
    if (it.right) {
      const s = clip(it.right, rightW - 1);
      g.text(g.w - [...s].length, row, s, it.tone === "bad" ? tone : p.dim);
    }
  }
}

function progress(g: Region, d: ProgressData): void {
  const p = g.p;
  let row = 0;
  const bw = Math.min(12, Math.max(4, Math.floor(g.w / 4)));
  for (const it of d.items ?? []) {
    const pct = (it.done / Math.max(it.total, 1)) * 100;
    g.bar(0, row, bw, pct, p.tone(it.tone ?? (pct >= 100 ? "good" : "busy")));
    const c = bw + g.text(bw, row, ` ${String(it.done).padStart(2)}/${String(it.total).padEnd(3)}`, p.tone("plain"), BOLD);
    g.text(c + 1, row, clip(it.label, g.w - c - 1), p.tone(it.tone === "bad" ? "bad" : "plain"));
    row++;
  }
  if (d.facts?.length) g.facts(0, ++row, d.facts), row++;
  if (d.lines?.length) text(new Region(g.buf, { ...g.r, y: g.r.y + row + 1, h: g.h - row - 1 }, g.p), { lines: d.lines });
}

function checks(g: Region, d: ChecksData): void {
  const p = g.p;
  let row = 0;
  for (const grp of d.groups ?? []) {
    const bad = grp.checks.filter((c) => !c.ok);
    const ok = grp.checks.filter((c) => c.ok);
    g.text(0, row, clip(grp.name, 14), p.tone("plain"), BOLD);
    g.text(15, row, `${ok.length}/${grp.checks.length} ok`, bad.length ? p.tone("bad") : p.tone("good"), bad.length ? BOLD : 0);
    row++;
    for (const c of bad) {
      const n = g.text(0, row, "!! ", p.tone("bad"), BOLD);
      const m = g.text(n, row, clip(c.name, 22), p.tone("plain"));
      if (c.detail) g.text(n + m + 2, row, clip(c.detail, g.w - n - m - 2), p.dim);
      row++;
    }
    const per = Math.max(1, Math.floor(g.w / 16));
    const cw = Math.floor(g.w / per);
    ok.forEach((c, i) => {
      const r = row + Math.floor(i / per);
      const col = (i % per) * cw;
      g.text(col, r, "■ ", p.tone("good"));
      g.text(col + 2, r, clip(c.name, cw - 3), p.dim);
    });
    row += Math.ceil(ok.length / per) + 1;
  }
}

function dots(g: Region, d: DotsData): void {
  const p = g.p;
  let row = 0;
  for (const grp of d.groups ?? []) {
    g.text(0, row, clip(grp.name, g.w), p.tone("plain"), BOLD);
    let c = 2;
    for (const u of grp.units) c += g.text(c, row + 1, "█ ", p.tone(u as Tone));
    if (grp.note) g.text(c + 1, row + 1, clip(grp.note, g.w - c - 1), p.dim);
    row += 3;
  }
}

function text(g: Region, d: TextData): void {
  for (const [row, l] of (d.lines ?? []).slice(0, g.h).entries()) {
    const f = typeof l === "string" ? { text: l } : l;
    g.text(0, row, clip(f.text, g.w), g.p.tone(f.tone ?? "plain"));
  }
}
