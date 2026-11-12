#!/usr/bin/env bun
import { launch, options } from "../../src/app.ts";
import { Dashboard } from "./dashboard.ts";
import { defaultSettingsPath, loadSettings } from "./settings.ts";

const HELP = `phosphor dashboard: a retro computer showing your status cards, with a screensaver.

  --settings FILE   cards and looks (default: ~/.config/phosphor/dashboard.json, else the example)
  --seconds N       quit after N seconds
  --sleep           start with the screensaver

After a while with no mouse or keys, a screensaver fills the window; move the mouse or press
a key to bring the dashboard back. Click a card to fill the screen with it; click again to go
back. Keys: space screensaver now, s screen colour, c case colour, 1-9 show one card,
esc all cards, q quit.`;

const opts = options(process.argv.slice(2));
if (opts.has("help")) {
  console.log(HELP);
  process.exit(0);
}
let loaded;
try {
  loaded = loadSettings(opts.get("settings") ?? defaultSettingsPath());
} catch (e) {
  console.error(`phosphor: ${(e as Error).message}`);
  process.exit(1);
}
const { settings, base } = loaded;
await launch({ seconds: Number(opts.get("seconds")) || undefined }, (renderer) => {
  const dash = new Dashboard(renderer, renderer, settings, base);
  renderer.root.add(dash);
  if (opts.has("sleep")) dash.sleep();
  return () => dash.stop();
});
process.exit(0);
