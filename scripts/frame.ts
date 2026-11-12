// Print one frame of the screensaver as text, without a terminal: bun scripts/frame.ts [cols] [rows] [settings]
import { createTestRenderer } from "@opentui/core/testing";
import { Screensaver } from "../apps/screensaver/screensaver.ts";
import { loadSettings } from "../apps/screensaver/settings.ts";

const [cols = "120", rows = "40", file] = process.argv.slice(2);
const { settings, base } = loadSettings(file ?? null);
const t = await createTestRenderer({ width: Number(cols), height: Number(rows) });
const saver = new Screensaver(t.renderer, t.renderer, settings, base);
t.renderer.root.add(saver);
await Bun.sleep(1500);
await t.renderOnce();
console.log(t.captureCharFrame());
saver.stop();
await t.dispose();
process.exit(0);
