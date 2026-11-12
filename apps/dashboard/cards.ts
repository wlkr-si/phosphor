import { TextAttributes, type OptimizedBuffer } from "@opentui/core";
import { mix, rgb } from "../../src/color.ts";
import type { Rect } from "./computer.ts";
import type { Palette } from "./theme.ts";
import type { CardSettings } from "./settings.ts";
import type { Reading } from "./sources.ts";
import type { Anim } from "./anim.ts";
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
const MARK = "■";

const width = (s: string) => [...s].length;

/** Shorten `s` to `n` cells, at a word break when one is near, ending in "…". */
export function clip(s: string, n: number): string {
  if (n <= 0) return "";
  const chars = [...s];
  if (chars.length <= n) return s;
  if (n === 1) return "…";
  const cut = chars.slice(0, n - 1).join("");
  const space = cut.lastIndexOf(" ");
  return (space >= n * 0.6 ? cut.slice(0, space) : cut).replace(/[\s,;:/-]+$/, "") + "…";
}

/** Break `s` into lines of at most `n` cells, at spaces where it can. */
export function wrap(s: string, n: number, maxLines: number): string[] {
  const out: string[] = [];
  let rest = s.trim();
  while (rest && out.length < maxLines && n > 0) {
    if (width(rest) <= n) {
      out.push(rest);
      break;
    }
    if (out.length === maxLines - 1) {
      out.push(clip(rest, n));
      break;
    }
    const chars = [...rest];
    let at = chars.slice(0, n + 1).join("").lastIndexOf(" ");
    if (at < n * 0.4) at = n;
    out.push(chars.slice(0, at).join("").trimEnd());
    rest = chars.slice(at).join("").trimStart();
  }
  return out;
}

/** A rectangle of the screen to write into, in its own coordinates, clipped to its edges. */
export class Region {
  constructor(
    readonly buf: OptimizedBuffer,
    readonly r: Rect,
    readonly p: Palette,
    readonly anim: Anim,
    readonly id: string,
  ) {}

  get w(): number {
    return this.r.w;
  }
  get h(): number {
    return this.r.h;
  }

  /** A smaller region inside this one. */
  sub(x: number, y: number, w: number, h: number): Region {
    const r = { x: this.r.x + x, y: this.r.y + y, w: Math.max(0, Math.min(w, this.r.w - x)), h: Math.max(0, Math.min(h, this.r.h - y)) };
    return new Region(this.buf, r, this.p, this.anim, this.id);
  }

  /** Write text at (col, row); returns how many cells it took. */
  text(col: number, row: number, s: string, color: string, attrs = 0, bg = this.p.screen): number {
    if (row < 0 || row >= this.r.h || col >= this.r.w || col < 0 || !s) return 0;
    const shown = [...s].slice(0, this.r.w - col).join("");
    if (!shown) return 0;
    this.buf.drawText(shown, this.r.x + col, this.r.y + row, rgb(color), rgb(bg), attrs);
    return width(shown);
  }

  /** Tint a row's background for a row that has just changed; returns the colour to write on. */
  glowRow(row: number, color: string, amount: number): string {
    if (amount <= 0 || row < 0 || row >= this.r.h) return this.p.screen;
    const bg = mix(this.p.screen, color, 0.3 * amount);
    this.buf.fillRect(this.r.x, this.r.y + row, this.r.w, 1, rgb(bg));
    return bg;
  }

  /** A line of facts, three spaces apart; a fact that does not fit whole is left off. */
  facts(col: number, row: number, facts: Fact[]): void {
    let c = col;
    for (const [i, f] of facts.entries()) {
      const gap = i > 0 ? 3 : 0;
      if (c + gap + width(f.text) > this.r.w) break;
      c += gap;
      c += this.text(c, row, f.text, this.p.tone(f.tone ?? "plain"));
    }
  }

  /** A bar `w` cells wide, filled to `pct` percent. Bars sit low in their cells so stacked bars stay apart. */
  bar(col: number, row: number, w: number, pct: number, color: string): void {
    if (w <= 0 || row < 0 || row >= this.r.h) return;
    const full = Math.round((Math.max(0, Math.min(100, pct)) / 100) * w);
    for (let i = 0; i < w && col + i < this.r.w; i++)
      this.buf.setCell(this.r.x + col + i, this.r.y + row, "▆", rgb(i < full ? color : this.p.track), rgb(this.p.screen));
  }
}

/** Green below 70%, amber below 90%, red above. */
function level(p: Palette, pct: number): string {
  return p.tone(pct < 70 ? "good" : pct < 90 ? "warn" : "bad");
}

function num(v: number): string {
  return v >= 100 || Number.isInteger(v) ? Math.round(v).toString() : v.toFixed(1);
}

/** An amount that changes: always one decimal below 100, so the text keeps its width as it moves. */
function moving(v: number): string {
  return v >= 100 ? Math.round(v).toString() : v.toFixed(1);
}

/** Resolve filled content and wrap text to the available cells before scrolling it. */
export function cardContent(card: CardSettings, reading: Reading, expanded: boolean, cells: number) {
  const content = expanded ? reading.data?.expanded : undefined;
  const kind = content?.type ?? card.type;
  let data = content?.data ?? reading.data;
  if (expanded && kind === "text" && data) {
    const lines = (data as TextData).lines;
    data = { lines: (Array.isArray(lines) ? lines : []).flatMap((line) => {
      const fact = typeof line === "string" ? { text: line } : line;
      if (!fact || typeof fact.text !== "string") return [];
      const indent = (fact.text.match(/^\s*/)?.[0] ?? "").slice(0, Math.max(0, cells - 1));
      const lines = wrap(fact.text, cells - indent.length, Number.MAX_SAFE_INTEGER);
      return (lines.length ? lines : [""]).map((text) => ({ ...fact, text: indent + text }));
    }) };
  }
  const rows = kind === "list" ? (data as ListData | null)?.items : kind === "text" ? (data as TextData | null)?.lines : undefined;
  return { kind, data, rows: Array.isArray(rows) ? rows : undefined };
}

/** Draw a card: a rounded border in its colour with its title, and its data inside. */
export function drawCard(buf: OptimizedBuffer, r: Rect, card: CardSettings, reading: Reading, p: Palette, anim: Anim, expanded = false, offset = 0): void {
  if (r.w < 8 || r.h < 3) return;
  const { kind, data: resolved, rows } = cardContent(card, reading, expanded, r.w - 4);
  let data = resolved;
  const start = Math.max(0, Math.min(offset, Math.max(0, (rows?.length ?? 0) - (r.h - 2))));
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
  const head = new Region(buf, { x: r.x + 2, y: r.y, w: r.w - 4, h: 1 }, p, anim, card.id);
  const c = head.text(0, 0, ` ${clip(card.title, r.w - 8)} `, accent, BOLD);
  if (reading.data && reading.error && c + 8 < head.w) head.text(c, 0, " stale ", p.tone("warn"));

  if (expanded && rows && rows.length > r.h - 2) {
    const range = ` ${start + 1}–${Math.min(rows.length, start + r.h - 2)}/${rows.length} `;
    if (c + range.length < head.w) head.text(head.w - range.length, 0, range, p.dim);
  }
  if (expanded && rows) data = kind === "list" ? { items: (rows as ListData["items"]).slice(start) } : { lines: (rows as TextData["lines"]).slice(start) };
  const inner = new Region(buf, { x: r.x + 2, y: r.y + 1, w: r.w - 4, h: r.h - 2 }, p, anim, card.id);
  if (!reading.data) {
    if (reading.error) wrap(`Can't read: ${reading.error}`, inner.w, inner.h).forEach((l, i) => inner.text(0, i, l, p.tone("bad")));
    else inner.text(0, 0, "Waiting for data…", p.dim);
    return;
  }
  try {
    DRAW[kind](inner, data as never, accent);
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
  const amount = (a: Amount, key: string) => {
    const used = g.anim.value(`${g.id}:${key}`, a.used);
    return { pct: (used / Math.max(a.total, 1e-9)) * 100, text: `${moving(used)}/${num(a.total)} ${a.unit ?? "GB"}` };
  };
  const rows: { label: string; pct: number; text: string }[] = [];
  if (typeof m.cpu === "number") {
    const cpu = g.anim.value(`${g.id}:cpu`, m.cpu);
    rows.push({ label: "CPU", pct: cpu, text: `${Math.round(cpu)}%` });
  }
  if (m.memory) rows.push({ label: "Memory", ...amount(m.memory, "mem") });
  if (m.disk) rows.push({ label: "Disk", ...amount(m.disk, "disk") });
  // Room for the widest value it could show, so the bars do not change length as numbers move.
  const widest = (a?: Amount) => (a ? width(`${num(a.total)}/${num(a.total)} ${a.unit ?? "GB"}`) : 0);
  const valueW = Math.max(4, widest(m.memory), widest(m.disk));
  const bw = Math.max(4, g.w - 8 - valueW - 2);
  let row = 0;
  for (const r of rows) {
    g.text(0, row, r.label, p.dim);
    g.bar(8, row, bw, r.pct, level(p, r.pct));
    g.text(g.w - width(r.text), row, r.text, p.tone("plain"), BOLD);
    row++;
  }
  if (m.facts?.length) g.facts(0, row++, m.facts);

  // The processor chart fills the rest, newest on the right.
  const top = row + 1;
  const rows2 = g.h - top;
  const hist = m.history ?? [];
  if (rows2 < 2 || g.w < 10) return;
  g.text(0, top, "100", p.faint);
  g.text(0, top + rows2 - 1, "  0", p.faint);
  const cw = g.w - 4;
  const blocks = " ▁▂▃▄▅▆▇";
  for (let i = 0; i < cw; i++) {
    const v = hist[hist.length - cw + i];
    for (let rr = 0; rr < rows2; rr++) {
      const x = g.r.x + 4 + i;
      const y = g.r.y + top + rows2 - 1 - rr;
      const eighths =
        v === undefined ? 0 : Math.max(0, Math.min(8, Math.round((Math.max(0, Math.min(100, v)) / 100) * rows2 * 8 - rr * 8)));
      if (eighths >= 8) g.buf.setCell(x, y, " ", rgb(accent), rgb(accent));
      else if (eighths > 0) g.buf.setCell(x, y, blocks[eighths]!, rgb(accent), rgb(p.screen));
      else g.buf.setCell(x, y, rr === 0 ? "▁" : " ", rgb(p.track), rgb(p.screen));
    }
  }
}

function list(g: Region, d: ListData, accent: string): void {
  const p = g.p;
  const items = d.items ?? [];
  if (!items.length) g.text(0, 0, "none yet", p.dim);
  // Columns sized to what they hold: tag, label, detail, then the note on the right.
  const tagW = Math.max(2, ...items.map((i) => (i.tag ? width(i.tag) + 2 : 2)));
  const rightW = Math.min(16, Math.max(0, ...items.map((i) => (i.right ? width(i.right) : 0))));
  const room = g.w - tagW - (rightW ? rightW + 2 : 0);
  const longestLabel = Math.max(0, ...items.map((i) => width(i.label)));
  const longestDetail = Math.max(0, ...items.map((i) => (i.detail ? width(i.detail) : 0)));
  const labelW = Math.min(longestLabel, Math.floor(room * 0.65));
  const showDetail = longestDetail > 0 && room - labelW - 2 >= Math.min(longestDetail, 8);
  const labelRoom = showDetail ? labelW : room;
  for (const [row, it] of items.slice(0, g.h).entries()) {
    const tone = p.tone(it.tone ?? "good");
    const bg = g.glowRow(row, accent, g.anim.glow(g.id, `${it.tag ?? ""}|${it.label}|${it.detail ?? ""}`));
    if (it.tag) g.text(0, row, it.tag, tone, it.tone === "bad" || it.tone === "busy" ? BOLD : 0, bg);
    else g.text(0, row, MARK, tone, 0, bg);
    g.text(tagW, row, clip(it.label, labelRoom), p.tone("plain"), 0, bg);
    if (showDetail && it.detail) g.text(tagW + labelW + 2, row, clip(it.detail, room - labelW - 2), p.dim, 0, bg);
    if (it.right) {
      const s = clip(it.right, rightW);
      g.text(g.w - width(s), row, s, it.tone === "bad" ? tone : p.dim, 0, bg);
    }
  }
}

function progress(g: Region, d: ProgressData, accent: string): void {
  const p = g.p;
  let row = 0;
  const bw = Math.min(12, Math.max(4, Math.floor(g.w / 5)));
  const countW = Math.max(5, ...(d.items ?? []).map((it) => width(`${it.done}/${it.total}`)));
  const labelX = bw + 1 + countW + 2;
  for (const it of d.items ?? []) {
    if (row >= g.h) break;
    const done = g.anim.value(`${g.id}:${it.label}`, it.done);
    const pct = (done / Math.max(it.total, 1)) * 100;
    const bg = g.glowRow(row, accent, g.anim.glow(g.id, `${it.label}|${it.done}`));
    g.bar(0, row, bw, pct, it.tone === "dim" ? p.dim : p.tone(it.tone ?? (pct >= 100 ? "good" : "busy")));
    const count = `${Math.round(done)}/${it.total}`;
    g.text(bw + 1 + countW - width(count), row, count, p.tone("plain"), BOLD, bg);
    const lines = wrap(it.label, g.w - labelX, 2);
    for (const [i, l] of lines.entries())
      g.text(labelX, row + i, l, it.tone === "dim" ? p.dim : p.tone(it.tone === "bad" ? "bad" : "plain"), 0, i ? p.screen : bg);
    row += Math.max(1, lines.length);
  }
  if (d.facts?.length) {
    g.facts(0, row + 1, d.facts);
    row += 2;
  }
  if (d.lines?.length) text(g.sub(0, row + 1, g.w, g.h - row - 1), { lines: d.lines }, accent);
}

function checks(g: Region, d: ChecksData): void {
  const p = g.p;
  let row = 0;
  for (const grp of d.groups ?? []) {
    if (row >= g.h) break;
    const bad = grp.checks.filter((c) => !c.ok);
    const ok = grp.checks.filter((c) => c.ok);
    const n = g.text(0, row, clip(grp.name, g.w - 12), p.tone("plain"), BOLD);
    g.text(n + 2, row, `${ok.length}/${grp.checks.length} ok`, bad.length ? p.tone("bad") : p.tone("good"), bad.length ? BOLD : 0);
    row++;
    for (const c of bad) {
      const bg = g.glowRow(row, p.tone("bad"), g.anim.glow(g.id, `${grp.name}|${c.name}|bad`));
      const m = g.text(0, row, "!! ", p.tone("bad"), BOLD, bg);
      const nm = g.text(m, row, clip(c.name, g.w - m), p.tone("plain"), 0, bg);
      const room = g.w - m - nm - 2;
      if (c.detail && room >= 12) g.text(m + nm + 2, row, clip(c.detail, room), p.dim, 0, bg);
      else if (c.detail && row + 1 < g.h) g.text(m, ++row, clip(c.detail, g.w - m), p.dim);
      row++;
    }
    // Passing checks flow along the line like words, each one whole.
    let col = 0;
    for (const c of ok) {
      const name = clip(c.name, g.w - 2);
      const need = 2 + width(name);
      if (col > 0 && col + need > g.w) {
        row++;
        col = 0;
      }
      const glow = g.anim.glow(g.id, `${grp.name}|${c.name}|ok`);
      const bg = glow > 0 ? mix(p.screen, p.tone("good"), 0.3 * glow) : p.screen;
      g.text(col, row, `${MARK} `, p.tone("good"), 0, bg);
      g.text(col + 2, row, name, p.dim, 0, bg);
      col += need + 2;
    }
    row += ok.length ? 2 : 1;
  }
}

function dots(g: Region, d: DotsData): void {
  const p = g.p;
  let row = 0;
  for (const grp of d.groups ?? []) {
    if (row >= g.h) break;
    g.text(0, row, clip(grp.name, g.w), p.tone("plain"), BOLD);
    let c = 2;
    for (const u of grp.units) c += g.text(c, row + 1, `${MARK} `, p.tone(u as Tone));
    if (grp.note && c + 1 + width(grp.note) <= g.w) g.text(c + 1, row + 1, grp.note, p.dim);
    row += 3;
  }
}

function text(g: Region, d: TextData, accent: string): void {
  for (const [row, l] of (d.lines ?? []).slice(0, g.h).entries()) {
    const f = typeof l === "string" ? { text: l } : l;
    const bg = g.glowRow(row, accent, g.anim.glow(`${g.id}:text`, f.text));
    g.text(0, row, clip(f.text, g.w), g.p.tone(f.tone ?? "plain"), 0, bg);
  }
}
