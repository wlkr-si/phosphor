import { TextAttributes, type OptimizedBuffer } from "@opentui/core";
import { bigText, bigWidth } from "../../src/bigtext.ts";
import { rgb } from "../../src/color.ts";
import type { Rect } from "./computer.ts";
import type { Palette } from "./theme.ts";
import type { CardSettings, Column, Row, Settings } from "./settings.ts";
import type { Reading } from "./sources.ts";
import { Region, drawCard } from "./cards.ts";
import type { Anim } from "./anim.ts";

/** Where each card sits on the screen, in cells. */
export interface Placed {
  card: CardSettings;
  rect: Rect;
}

/** Split `total` cells by `shares`, giving any remainder to the last. */
function split(total: number, shares: number[]): number[] {
  const sum = shares.reduce((a, b) => a + b, 0) || 1;
  const out = shares.map((s) => Math.floor((total * s) / sum));
  out[out.length - 1]! += total - out.reduce((a, b) => a + b, 0);
  return out;
}

/** Cards in rows of up to three, when the settings give no layout. */
function autoLayout(cards: CardSettings[]): Row[] {
  const rows: Row[] = [];
  for (let i = 0; i < cards.length; i += 3) rows.push({ columns: cards.slice(i, i + 3).map((c) => [c.id]) });
  return rows;
}

/** Place the cards inside `area`. Cards the layout leaves out get a row of their own at the end. */
export function place(area: Rect, s: Settings): Placed[] {
  const byId = new Map(s.cards.map((c) => [c.id, c]));
  const rows = (s.layout ?? autoLayout(s.cards)).map((r) => ({
    height: r.height ?? 1,
    columns: r.columns.map((c: Column) =>
      Array.isArray(c) ? { width: 1, cards: c, heights: [] as number[] } : { width: c.width ?? 1, cards: c.cards, heights: c.heights ?? [] },
    ),
  }));
  const used = new Set(rows.flatMap((r) => r.columns.flatMap((c) => c.cards)));
  const left = s.cards.filter((c) => !used.has(c.id));
  if (left.length) rows.push({ height: 1, columns: left.map((c) => ({ width: 1, cards: [c.id], heights: [] })) });

  const out: Placed[] = [];
  let y = area.y;
  for (const [ri, rh] of split(area.h, rows.map((r) => r.height)).entries()) {
    const row = rows[ri]!;
    let x = area.x;
    for (const [ci, cw] of split(area.w, row.columns.map((c) => c.width)).entries()) {
      const col = row.columns[ci]!;
      const ids = col.cards.filter((id) => byId.has(id));
      const shares = ids.map((id) => col.heights[col.cards.indexOf(id)] ?? 1);
      let cy = y;
      for (const [ki, ch] of split(rh, shares).entries()) {
        out.push({ card: byId.get(ids[ki]!)!, rect: { x, y: cy, w: cw, h: ch } });
        cy += ch;
      }
      x += cw;
    }
    y += rh;
  }
  return out;
}

/** The inside of the screen: a title and clock, a rule, then the cards (or one card, zoomed). */
export function drawBoard(
  buf: OptimizedBuffer,
  screen: Rect,
  s: Settings,
  p: Palette,
  read: (c: CardSettings) => Reading,
  now: Date,
  zoomed: string | null,
  notice: string | null,
  anim: Anim,
): Placed[] {
  buf.fillRect(screen.x, screen.y, screen.w, screen.h, rgb(p.screen));
  const inner = { x: screen.x + 2, y: screen.y + 1, w: screen.w - 4, h: screen.h - 2 };
  if (inner.w < 10 || inner.h < 4) return [];
  const clock = notice ?? now.toTimeString().slice(0, 8);
  // Big letters for the title and clock when there is room, as on the old status screens.
  const title = s.title.toUpperCase();
  const big = inner.h >= 16 && bigWidth(title) + bigWidth(clock) + 4 <= inner.w;
  const headH = big ? 2 : 1;
  if (big) {
    bigText(buf, title, inner.x, inner.y, p.ink, p.screen);
    bigText(buf, clock, inner.x + inner.w - bigWidth(clock), inner.y, p.clock, p.screen);
  } else {
    const head = new Region(buf, { ...inner, h: 1 }, p, anim, "head");
    head.text(0, 0, s.title.slice(0, Math.max(0, inner.w - clock.length - 2)), p.ink, TextAttributes.BOLD);
    head.text(inner.w - clock.length, 0, clock, p.clock, TextAttributes.BOLD);
  }
  for (let x = inner.x; x < inner.x + inner.w; x++) buf.setCell(x, inner.y + headH, "─", rgb(p.faint), rgb(p.screen));

  const area = { x: inner.x, y: inner.y + headH + 1, w: inner.w, h: inner.h - headH - 1 };
  const zoomedCard = zoomed ? s.cards.find((c) => c.id === zoomed) : undefined;
  const placed = zoomedCard ? [{ card: zoomedCard, rect: area }] : place(area, s);
  for (const pl of placed) drawCard(buf, pl.rect, pl.card, read(pl.card), p, anim);
  return placed;
}
