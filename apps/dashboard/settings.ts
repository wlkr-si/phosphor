import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import example from "./example.json" with { type: "json" };

/** Where a card's data comes from. Cards with the same source share one fetch. */
export type Source =
  | { demo: string }
  | { system: true; every?: number }
  | { url: string; path?: string; every?: number; headers?: Record<string, string> }
  | { file: string; path?: string; every?: number }
  | { command: string; path?: string; every?: number };

export type CardType = "machine" | "list" | "progress" | "checks" | "dots" | "text";

export interface CardSettings {
  id: string;
  type: CardType;
  title: string;
  /** The border colour: green, cyan, magenta, amber, blue, red or white. */
  color?: string;
  source: Source;
}

/** A column of the layout: cards stacked top to bottom, sharing its height by `heights` (equal by default). */
export type Column = string[] | { width?: number; cards: string[]; heights?: number[] };
/** A row of the layout. Heights and widths are shares of the room, not cells. */
export interface Row {
  height?: number;
  columns: Column[];
}

export interface Settings {
  /** Big text at the top left of the screen. */
  title: string;
  /** Small text on the computer's chin. */
  label: string;
  /** Seconds with no mouse or keys before the screensaver starts. */
  idle: number;
  /** Seconds the screensaver runs before the dashboard comes back. 0 turns it off. */
  screensaver: number;
  /** The big title the screensaver shows. Empty for the label. */
  saverText: string;
  /** Which screensaver effects to use: rain, decrypt, beams, swarm. Empty for all. */
  effects: string[];
  /** "color", "green", "amber" or "white": the screen's ink. */
  screen: string;
  /** "beige", "graphite" or "white". */
  case: string;
  /** The computer's share of the window, 0.5 to 1. */
  size: number;
  /** "23:00-07:00" blanks the screen overnight. Empty for never. */
  quietHours: string;
  cards: CardSettings[];
  /** Rows of columns of cards. Leave it out to place cards automatically. */
  layout?: Row[];
}

const DEFAULTS: Omit<Settings, "cards"> = {
  title: "PHOSPHOR STATUS",
  label: "PHOSPHOR",
  idle: 30,
  screensaver: 30,
  saverText: "",
  effects: [],
  screen: "color",
  case: "beige",
  size: 1,
  quietHours: "",
};

/** The settings file used when none is named: the first that exists. */
export function defaultSettingsPath(): string | null {
  const xdg = process.env.XDG_CONFIG_HOME || join(homedir(), ".config");
  const p = join(xdg, "phosphor", "dashboard.json");
  return existsSync(p) ? p : null;
}

/** Read settings from a file, or the built-in example when `path` is null. */
export function loadSettings(path: string | null): { settings: Settings; base: string } {
  if (!path) return { settings: normalise(example as unknown as Partial<Settings>), base: process.cwd() };
  const full = resolve(path);
  let raw: unknown;
  try {
    raw = JSON.parse(readFileSync(full, "utf8"));
  } catch (e) {
    throw new Error(`could not read ${full}: ${(e as Error).message}`);
  }
  return { settings: normalise(raw as Partial<Settings>), base: dirname(full) };
}

const TYPES: CardType[] = ["machine", "list", "progress", "checks", "dots", "text"];

function normalise(raw: Partial<Settings>): Settings {
  if (typeof raw !== "object" || raw === null) throw new Error("settings must be a JSON object");
  const s: Settings = { ...DEFAULTS, ...raw, cards: [] } as Settings;
  s.size = Math.min(1, Math.max(0.5, Number(s.size) || DEFAULTS.size));
  s.idle = Math.max(1, Number(s.idle) || DEFAULTS.idle);
  s.screensaver = Math.max(0, Number(s.screensaver ?? DEFAULTS.screensaver) || 0);
  s.effects = Array.isArray(s.effects) ? s.effects.map(String) : [];
  const seen = new Set<string>();
  for (const [i, c] of (raw.cards ?? []).entries()) {
    if (!c || typeof c !== "object") throw new Error(`card ${i + 1} must be an object`);
    if (!TYPES.includes(c.type)) throw new Error(`card ${i + 1}: type must be one of ${TYPES.join(", ")}`);
    if (!c.source || typeof c.source !== "object") throw new Error(`card ${i + 1}: needs a source`);
    const id = String(c.id ?? `card-${i + 1}`);
    if (seen.has(id)) throw new Error(`two cards are called ${id}`);
    seen.add(id);
    s.cards.push({ ...c, id, title: String(c.title ?? id) });
  }
  return s;
}
