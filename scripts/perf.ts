// How long the screensaver takes to draw, without a terminal: bun scripts/perf.ts [cols] [rows] [settings]
import { createTestRenderer } from "@opentui/core/testing";
import { Screensaver } from "../apps/screensaver/screensaver.ts";
import { loadSettings } from "../apps/screensaver/settings.ts";

const [cols = "263", rows = "67", file] = process.argv.slice(2);
const { settings, base } = loadSettings(file ?? null);
const t = await createTestRenderer({ width: Number(cols), height: Number(rows) });
const saver = new Screensaver(t.renderer, t.renderer, settings, base);
t.renderer.root.add(saver);
await Bun.sleep(1500);
const times: number[] = [];
const start = performance.now();
const x0 = saver.inspect().computer;
while (performance.now() - start < 4000) {
  const a = performance.now();
  await t.renderOnce();
  times.push(performance.now() - a);
  await Bun.sleep(1000 / 30);
}
times.sort((a, b) => a - b);
const x1 = saver.inspect().computer;
console.log(`frames ${times.length}  median ${times[times.length >> 1]!.toFixed(1)} ms  worst ${times[times.length - 1]!.toFixed(1)} ms`);
console.log(`moved from ${x0.x},${x0.y} to ${x1.x},${x1.y}`);
saver.stop();
await t.dispose();
process.exit(0);
