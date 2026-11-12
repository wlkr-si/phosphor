import { afterEach, expect, test } from "bun:test";
import { createTestRenderer, type TestRendererSetup } from "@opentui/core/testing";
import { Screensaver, quiet } from "./screensaver.ts";
import { loadSettings, type Settings } from "./settings.ts";
import { place } from "./board.ts";
import { pick } from "./sources.ts";
import { Bounce } from "../../src/bounce.ts";

let open: { t: TestRendererSetup; saver: Screensaver } | null = null;
afterEach(async () => {
  open?.saver.stop();
  await open?.t.dispose();
  open = null;
});

async function start(edit: (s: Settings) => void = () => {}, w = 160, h = 50) {
  const { settings, base } = loadSettings(null);
  edit(settings);
  const t = await createTestRenderer({ width: w, height: h, useMouse: true });
  const saver = new Screensaver(t.renderer, t.renderer, settings, base);
  t.renderer.root.add(saver);
  open = { t, saver };
  await t.renderOnce();
  return open;
}

test("the example shows its title, clock and every card", async () => {
  const { t, saver } = await start();
  const frame = t.captureCharFrame();
  for (const title of ["This computer", "Server", "Workers", "Services", "Recent deploys", "Projects", "Health"])
    expect(frame).toContain(title);
  expect(frame).toContain("PHOSPHOR");
  expect(saver.inspect().cards.length).toBe(7);
});

test("dragging moves the computer, and it stays inside the window", async () => {
  const { t, saver } = await start((s) => (s.speed = 0));
  const before = saver.inspect().computer;
  await t.mockMouse.drag(before.x + 5, before.y + before.h - 2, before.x + 200, before.y + before.h + 50);
  await t.renderOnce();
  const after = saver.inspect().computer;
  expect(after.x).toBeGreaterThan(before.x);
  expect(after.x + after.w).toBeLessThanOrEqual(160);
  expect(after.y + after.h).toBeLessThanOrEqual(50);
});

test("clicking a card fills the screen with it; clicking again goes back", async () => {
  const { t, saver } = await start((s) => (s.speed = 0));
  const card = saver.inspect().cards.find((c) => c.card.id === "deploys")!;
  await t.mockMouse.click(card.rect.x + 3, card.rect.y + 2);
  await t.renderOnce();
  expect(saver.inspect().zoomed).toBe("deploys");
  expect(saver.inspect().cards.length).toBe(1);
  await t.mockMouse.click(card.rect.x + 3, card.rect.y + 2);
  await t.renderOnce();
  expect(saver.inspect().zoomed).toBeNull();
});

test("scrolling and + and - change the speed; space pauses", async () => {
  const { t, saver } = await start();
  const s0 = saver.inspect().speed;
  await t.mockMouse.scroll(80, 25, "up");
  expect(saver.inspect().speed).toBeGreaterThan(s0);
  t.mockInput.pressKey("-");
  t.mockInput.pressKey("-");
  expect(saver.inspect().speed).toBeLessThan(s0 * 1.25);
  t.mockInput.pressKey(" ");
  expect(saver.inspect().paused).toBe(true);
});

test("a small window still draws without errors", async () => {
  const { t } = await start(undefined, 40, 18);
  expect(t.captureCharFrame().length).toBeGreaterThan(0);
});

test("layouts share the room, with uneven heights in a column", () => {
  const { settings } = loadSettings(null);
  settings.layout = [{ columns: [{ cards: ["workers", "services"], heights: [1, 3] }] }];
  const placed = place({ x: 0, y: 0, w: 100, h: 40 }, settings);
  const workers = placed.find((p) => p.card.id === "workers")!;
  const services = placed.find((p) => p.card.id === "services")!;
  // The five cards left out take a second row, so this column gets half the height.
  expect(workers.rect.h).toBe(5);
  expect(services.rect.h).toBe(15);
  // Cards the layout leaves out still get a place.
  expect(placed.length).toBe(settings.cards.length);
});

test("quiet hours can run past midnight", () => {
  const at = (h: number, m = 0) => new Date(2026, 0, 1, h, m);
  expect(quiet("23:00-07:00", at(23, 30))).toBe(true);
  expect(quiet("23:00-07:00", at(3))).toBe(true);
  expect(quiet("23:00-07:00", at(12))).toBe(false);
  expect(quiet("", at(3))).toBe(false);
});

test("paths reach into data", () => {
  expect(pick({ a: [{ b: 2 }] }, "a[0].b")).toBe(2);
  expect(pick({ a: 1 }, "x.y")).toBeUndefined();
});

test("a bounce never leaves the room", () => {
  const b = new Bounce(30);
  for (let i = 0; i < 2000; i++) {
    b.step(1 / 30, 17, 5);
    expect(b.x).toBeGreaterThanOrEqual(0);
    expect(b.x).toBeLessThanOrEqual(17);
    expect(b.y).toBeGreaterThanOrEqual(0);
    expect(b.y).toBeLessThanOrEqual(5);
  }
  b.fling(500, -300);
  for (let i = 0; i < 300; i++) b.step(1 / 30, 17, 5);
  expect(Math.hypot(b.vx, b.vy * 2)).toBeCloseTo(30, 0);
});

import { placement, blockSteps } from "./computer.ts";
import { clip, wrap } from "./cards.ts";
import { Anim } from "./anim.ts";
import { paintShapes } from "../../src/shapes.ts";
import { OptimizedBuffer } from "@opentui/core";

test("the case sits at eighths of a cell, the screen's text on whole cells", () => {
  const p = placement(10.3, 4.9);
  expect([p.cellX, p.qx, p.cellY, p.qy]).toEqual([10, 2, 4, 7]);
  expect([p.snapX, p.snapY]).toEqual([10, 5]);
  // On whole cells only (Apple's Terminal), there are no eighths.
  const w = placement(10.3, 4.9, 1);
  expect([w.qx, w.qy, w.cellX, w.cellY]).toEqual([0, 0, 10, 5]);
  expect(blockSteps({ TERM_PROGRAM: "Apple_Terminal" })).toBe(1);
  expect(blockSteps({ TERM_PROGRAM: "ghostty" })).toBe(8);
});

test("a rectangle edge inside a cell becomes a partial block", async () => {
  const t = await createTestRenderer({ width: 4, height: 2 });
  const buf = OptimizedBuffer.create(3, 2, t.renderer.widthMethod, { owner: t.renderer.nativeScene });
  paintShapes(buf, [{ x: 0, y: 0.625, w: 1.375, h: 2, color: "#c9c1a9" }], "#000000");
  const text = new TextDecoder().decode(buf.getRealCharBytes(true));
  // Top row: the lower 3/8 filled. Second row: the left 3/8 of the right-hand cell.
  expect(text.split("\n")[0]).toContain("▃");
  expect(text.split("\n")[1]).toContain("▍");
  buf.destroy();
  await t.dispose();
});

test("glides start and end gently, rest between, and never leave the room", () => {
  const b = new Bounce(1.5);
  // Pace follows a sine: zero at the ends of a glide.
  const paces = [0, 0.25, 0.5, 0.75, 1].map((u) => Math.sin(Math.PI * u));
  expect(paces[0]).toBeCloseTo(0);
  expect(paces[2]).toBeCloseTo(1);
  b.step(1, 20, 5, 0);
  expect([b.x, b.y]).toEqual([0, 0]);
});

test("words are cut at a space when one is near, and wrapped onto lines", () => {
  expect(clip("paused: make WLKR.SI a remarkable software studio", 24)).toBe("paused: make WLKR.SI a…");
  expect(clip("short", 10)).toBe("short");
  expect(wrap("make WLKR.SI a remarkable software studio by November", 24, 2)).toEqual([
    "make WLKR.SI a",
    "remarkable software…",
  ]);
});

test("changed values slide; new rows glow, but the first drawing does not", () => {
  const a = new Anim();
  a.frame(0);
  expect(a.value("cpu", 10)).toBe(10);
  expect(a.glow("jobs", "one")).toBe(0);
  expect(a.glow("jobs", "two")).toBe(0);
  a.frame(100);
  const mid = a.value("cpu", 50);
  expect(mid).toBe(10);
  a.frame(500);
  const later = a.value("cpu", 50);
  expect(later).toBeGreaterThan(10);
  expect(later).toBeLessThan(50);
  expect(a.glow("jobs", "three")).toBeGreaterThan(0.9);
  expect(a.busy).toBe(true);
  a.frame(5000);
  expect(a.value("cpu", 50)).toBe(50);
  expect(a.glow("jobs", "three")).toBe(0);
});
