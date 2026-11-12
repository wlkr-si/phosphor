// Print one frame of the dashboard as text, without a terminal: bun scripts/frame.ts [cols] [rows] [settings]
import { createTestRenderer } from "@opentui/core/testing";
import { Dashboard } from "../apps/dashboard/dashboard.ts";
import { loadSettings } from "../apps/dashboard/settings.ts";

const [cols = "120", rows = "40", file] = process.argv.slice(2);
const { settings, base } = loadSettings(file ?? null);
const t = await createTestRenderer({ width: Number(cols), height: Number(rows) });
const saver = new Dashboard(t.renderer, t.renderer, settings, base);
t.renderer.root.add(saver);
await Bun.sleep(1500);
await t.renderOnce();
console.log(t.captureCharFrame());
saver.stop();
await t.dispose();
process.exit(0);
