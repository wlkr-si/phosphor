#!/usr/bin/env bun
import { launch, options } from "../../src/app.ts";
import { Screensaver } from "./screensaver.ts";
import { defaultSettingsPath, loadSettings } from "./settings.ts";

const HELP = `phosphor screensaver: a retro computer gliding around the terminal, showing your status cards.

  --settings FILE   cards and looks (default: ~/.config/phosphor/screensaver.json, else the example)
  --seconds N       quit after N seconds

Drag the computer to move it, and let go mid-drag to throw it. Click a card to fill the
screen with it; click again to go back. Scroll or + and - change the speed.
Keys: space pause, m glide or drift, s screen colour, c case colour, 1-9 show one card,
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
  const saver = new Screensaver(renderer, renderer, settings, base);
  renderer.root.add(saver);
  return () => saver.stop();
});
process.exit(0);
