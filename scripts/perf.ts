// How long the dashboard takes to draw, without a terminal: bun scripts/perf.ts [cols] [rows] [settings] [--sleep]
// --sleep times the screensaver instead.
import { createTestRenderer } from "@opentui/core/testing";
import { Dashboard } from "../apps/dashboard/dashboard.ts";
import { loadSettings } from "../apps/dashboard/settings.ts";

const [cols = "263", rows = "67", file] = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const { settings, base } = loadSettings(file ?? null);
const t = await createTestRenderer({ width: Number(cols), height: Number(rows) });
const saver = new Dashboard(t.renderer, t.renderer, settings, base);
t.renderer.root.add(saver);
await Bun.sleep(1500);
const times: number[] = [];
const start = performance.now();
if (process.argv.includes("--sleep")) saver.sleep();
while (performance.now() - start < 4000) {
  const a = performance.now();
  await t.renderOnce();
  times.push(performance.now() - a);
  await Bun.sleep(1000 / 30);
}
times.sort((a, b) => a - b);
console.log(`frames ${times.length}  median ${times[times.length >> 1]!.toFixed(1)} ms  worst ${times[times.length - 1]!.toFixed(1)} ms`);
saver.stop();
await t.dispose();
process.exit(0);
