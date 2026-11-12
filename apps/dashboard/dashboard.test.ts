import { afterEach, expect, test } from "bun:test";
import { OptimizedBuffer } from "@opentui/core";
import { createTestRenderer, type TestRendererSetup } from "@opentui/core/testing";
import { Dashboard, quiet } from "./dashboard.ts";
import { loadSettings, type Pictures, type Settings } from "./settings.ts";
import { place } from "./board.ts";
import { pick } from "./sources.ts";
import { clip, wrap } from "./cards.ts";
import { Anim } from "./anim.ts";
import { Saver, EFFECT_NAMES, EFFECT_SECONDS } from "../../src/saver.ts";
import { pixels } from "../../src/pixelfont.ts";
import { artPixels, parseArt } from "../../src/art.ts";
import { blockWidth } from "../../src/blocktext.ts";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

let open: { t: TestRendererSetup; dash: Dashboard } | null = null;
afterEach(async () => {
  open?.dash.stop();
  await open?.t.dispose();
  open = null;
});

async function start(edit: (s: Settings) => void = () => {}, w = 160, h = 50, pictures?: Pictures) {
  const { settings, base } = loadSettings(null);
  edit(settings);
  const t = await createTestRenderer({ width: w, height: h, useMouse: true });
  const dash = new Dashboard(t.renderer, t.renderer, settings, base, pictures);
  t.renderer.root.add(dash);
  open = { t, dash };
  await t.renderOnce();
  return open;
}

test("the example shows its title, clock and every card", async () => {
  const { t, dash } = await start();
  const frame = t.captureCharFrame();
  for (const title of ["This computer", "Server", "Workers", "Services", "Recent deploys", "Projects", "Health"])
    expect(frame).toContain(title);
  expect(frame).toContain("PHOSPHOR");
  expect(dash.inspect().cards.length).toBe(7);
});

test("after the idle time the screensaver takes over, and any input brings the dashboard back", async () => {
  const { t, dash } = await start((s) => {
    s.idle = 1;
    s.screensaver = 30;
  });
  expect(dash.inspect().mode).toBe("board");
  await Bun.sleep(1700);
  expect(dash.inspect().mode).toBe("saver");
  // The first report only says where the pointer is; moving from there is someone using it.
  await t.mockMouse.moveTo(10, 10);
  await t.mockMouse.moveTo(12, 11);
  expect(["to-board", "board"]).toContain(dash.inspect().mode);
  await Bun.sleep(600);
  expect(dash.inspect().mode).toBe("board");
});

test("moving the mouse keeps the screensaver away", async () => {
  const { t, dash } = await start((s) => (s.idle = 1));
  for (let i = 0; i < 4; i++) {
    await Bun.sleep(400);
    await t.mockMouse.moveTo(20 + i, 10);
  }
  expect(dash.inspect().mode).toBe("board");
});

test("space starts the screensaver; a key that wakes does nothing else", async () => {
  const { t, dash } = await start();
  t.mockInput.pressKey(" ");
  expect(dash.inspect().mode).toBe("to-saver");
  t.mockInput.pressKey("2");
  expect(dash.inspect().mode).toBe("to-board");
  expect(dash.inspect().zoomed).toBeNull();
});

test("clicking a card fills the screen with it; clicking again goes back", async () => {
  const { t, dash } = await start();
  const card = dash.inspect().cards.find((c) => c.card.id === "deploys")!;
  await t.mockMouse.click(card.rect.x + 3, card.rect.y + 2);
  await t.renderOnce();
  expect(dash.inspect().zoomed).toBe("deploys");
  expect(dash.inspect().cards.length).toBe(1);
  await t.mockMouse.click(card.rect.x + 3, card.rect.y + 2);
  await t.renderOnce();
  expect(dash.inspect().zoomed).toBeNull();
});

test("a small window still draws without errors", async () => {
  const { t } = await start(undefined, 40, 18);
  expect(t.captureCharFrame().length).toBeGreaterThan(0);
});

test("every screensaver effect draws the title somewhere on screen, and clears it by the end", async () => {
  const t = await createTestRenderer({ width: 120, height: 40 });
  const buf = OptimizedBuffer.create(120, 40, t.renderer.widthMethod, { owner: t.renderer.nativeScene });
  const colours = { bg: "#000000", ink: "#7dff8f", glow: "#eafff0", dim: "#5f8f6d", accents: ["#55e2ff", "#ff70d8", "#ffb43c"] };
  for (const name of EFFECT_NAMES) {
    const saver = new Saver("HI", [name]);
    saver.start(0);
    const lit = (secs: number) => {
      saver.draw(buf, secs * 1000, colours);
      let n = 0;
      buf.withBuffers((b) => {
        for (let i = 0; i < 120 * 40; i++) if (b.bg[i * 4 + 1]! > 0) n++;
      });
      return n;
    };
    expect(lit(6)).toBeGreaterThan(50);
    expect(lit(EFFECT_SECONDS - 0.05)).toBeLessThan(lit(6));
  }
  buf.destroy();
  await t.dispose();
});

test("the pixel alphabet spells words", () => {
  const { cols, rows, pixels: px } = pixels("HI");
  expect([cols, rows]).toEqual([11, 7]);
  expect(px.length).toBe(17 + 11);
});

test("pictures drawn in block characters become square pixels, two to a cell", () => {
  const art = parseArt("\n█▀ \n ▄█\n\n");
  expect([art.w, art.h]).toEqual([3, 2]);
  const { pixels: px, cols, rows } = artPixels(art);
  expect([cols, rows]).toEqual([3, 4]);
  const on = new Set(px.map((p) => `${p.col},${p.row}`));
  expect([...on].sort()).toEqual(["0,0", "0,1", "1,0", "1,3", "2,2", "2,3"]);
});

test("block letters are whole cells, two to a pixel, with narrow colons", () => {
  expect(blockWidth("1")).toBe(6);
  expect(blockWidth("1:2")).toBe(6 + 2 + 2 + 2 + 6);
  expect(blockWidth("10:48:12")).toBe(6 * 6 + 2 * 2 + 7 * 2);
});

test("the screensaver shows its pictures in turn, leaving out any too wide for the window", () => {
  const wide = artPixels(parseArt("█".repeat(70)));
  const small = artPixels(parseArt("██\n██"));
  const saver = new Saver([wide, "HI", small]);
  expect([0, 1, 2].map((r) => saver.titleFor(r, 200, 50))).toEqual([0, 1, 2]);
  expect([0, 1, 2].map((r) => saver.titleFor(r, 100, 50))).toEqual([1, 2, 1]);
});

test("a logo takes the title's place when there is room, read from beside the settings file", async () => {
  const dir = mkdtempSync(join(tmpdir(), "phosphor-test-"));
  try {
    writeFileSync(join(dir, "logo.txt"), "▄▀▄▀▄▀▄▀▄▀\n██████████\n▀▄▀▄▀▄▀▄▀▄\n");
    writeFileSync(join(dir, "s.json"), JSON.stringify({ logo: "logo.txt", saverArt: ["logo.txt"], cards: [] }));
    const { pictures } = loadSettings(join(dir, "s.json"));
    expect(pictures.logo?.h).toBe(3);
    expect(pictures.saver.length).toBe(1);
    const { t } = await start(undefined, 160, 50, pictures);
    expect(t.captureCharFrame()).toContain("▄▀▄▀▄▀▄▀▄▀");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
  expect(() => loadSettings(join(dir, "s.json"))).toThrow();
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
  expect(a.value("cpu", 50)).toBe(10);
  a.frame(500);
  const later = a.value("cpu", 50);
  expect(later).toBeGreaterThan(10);
  expect(later).toBeLessThan(50);
  expect(a.glow("jobs", "three")).toBeGreaterThan(0.9);
  a.frame(5000);
  expect(a.value("cpu", 50)).toBe(50);
  expect(a.glow("jobs", "three")).toBe(0);
});

test("values that change every half second slide only briefly, so each tick shows", () => {
  const a = new Anim();
  a.frame(0);
  a.value("cpu", 10);
  a.frame(1000);
  a.value("cpu", 20); // the first change after a long rest slides slowly
  a.frame(1500);
  a.value("cpu", 30); // half a second later: a quick slide
  a.frame(1750);
  expect(a.value("cpu", 30)).toBe(30);
});

test("filled content replaces the summary; every row is reachable and leaving resets the view", async () => {
  const dir = mkdtempSync(join(tmpdir(), "phosphor-detail-"));
  try {
    writeFileSync(join(dir, "data.json"), JSON.stringify({ items: [{ label: "summary only" }], expanded: {
      type: "text", data: { lines: Array.from({ length: 120 }, (_, i) => `detail row ${String(i).padStart(3, "0")}`) },
    } }));
    const { t, dash } = await start((s) => {
      s.cards = [{ id: "detail", type: "list", title: "Detail", color: "green", source: { file: join(dir, "data.json") } }];
      s.layout = undefined;
    });
    await Bun.sleep(40);
    await t.renderOnce();
    expect(t.captureCharFrame()).toContain("summary only");
    expect(t.captureCharFrame()).not.toContain("detail row");
    const r = dash.inspect().cards[0]!.rect;
    await t.mockMouse.click(r.x + 3, r.y + 2);
    await t.renderOnce();
    expect(t.captureCharFrame()).toContain("detail row 000");
    expect(t.captureCharFrame()).not.toContain("summary only");
    await t.mockMouse.scroll(r.x + 3, r.y + 2, "down");
    await t.renderOnce();
    expect(dash.inspect().zoomed).toBe("detail");
    expect(t.captureCharFrame()).toContain("detail row 003");
    expect(t.captureCharFrame()).not.toContain("detail row 000");
    t.mockInput.pressKey("END");
    await t.renderOnce();
    expect(t.captureCharFrame()).toContain("detail row 119");
    expect(t.captureCharFrame()).not.toContain("detail row 000");
    t.mockInput.pressKey("HOME");
    await t.renderOnce();
    expect(t.captureCharFrame()).toContain("detail row 000");
    t.mockInput.pressKey("ESCAPE");
    await Bun.sleep(50);
    await t.renderOnce();
    expect(t.captureCharFrame()).toContain("summary only");
    t.mockInput.pressKey("1");
    await t.renderOnce();
    expect(t.captureCharFrame()).toContain("detail row 000");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
