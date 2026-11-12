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
