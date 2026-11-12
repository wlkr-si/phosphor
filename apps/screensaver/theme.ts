import type { Tone } from "./data.ts";
import { mix } from "../../src/color.ts";

/** Every colour the screensaver uses, as "#rrggbb". */
export interface Palette {
  room: string;
  case: string;
  caseLight: string;
  caseShade: string;
  caseDark: string;
  screen: string;
  ink: string;
  dim: string;
  faint: string;
  track: string;
  clock: string;
  accent: (name: string | undefined) => string;
  tone: (t: Tone | undefined) => string;
}

const ROOM = "#08090b";

export const SCREENS = ["color", "green", "amber", "white"] as const;
export const CASES = ["beige", "graphite", "white", "rainbow"] as const;

/** Old plastics for the rainbow case: a new one on every bounce. */
const RAINBOW = ["#c9c1a9", "#a9c4b0", "#a9b8c9", "#c0aac6", "#c9aaa6", "#c8c29c"];

const CASE_COLOURS: Record<string, string> = { beige: "#c9c1a9", graphite: "#55585e", white: "#dedcd4" };

/** The colours of a phosphor screen. Monochrome screens keep tones apart by brightness. */
function screenColours(screen: string): Omit<Palette, "room" | "case" | "caseLight" | "caseShade" | "caseDark"> {
  if (screen === "color" || !["green", "amber", "white"].includes(screen)) {
    const accents: Record<string, string> = {
      green: "#5dff86",
      cyan: "#55e2ff",
      magenta: "#ff70d8",
      amber: "#ffb43c",
      blue: "#7aa2ff",
      red: "#ff5f5f",
      white: "#e9efe9",
    };
    const tones: Record<Tone, string> = {
      good: "#5dff86",
      bad: "#ff5f5f",
      busy: "#55e2ff",
      warn: "#ffb43c",
      dim: "#5f8f6d",
      plain: "#d3f5dc",
    };
    return {
      screen: "#06120b",
      ink: "#7dff8f",
      dim: "#5f8f6d",
      faint: "#2c4a36",
      track: "#163222",
      clock: "#ffb43c",
      accent: (n) => accents[n ?? "green"] ?? accents.green!,
      tone: (t) => tones[t ?? "plain"],
    };
  }
  const hue = { green: "#66ff8c", amber: "#ffb02e", white: "#e8ecf0" }[screen]!;
  const bg = { green: "#04110a", amber: "#120b02", white: "#0b0c0e" }[screen]!;
  const at = (k: number) => mix(bg, hue, k);
  const tones: Record<Tone, string> = { good: at(0.85), bad: at(1), busy: at(0.95), warn: at(1), dim: at(0.5), plain: at(0.8) };
  return {
    screen: bg,
    ink: at(1),
    dim: at(0.5),
    faint: at(0.25),
    track: at(0.14),
    clock: at(1),
    accent: () => at(0.75),
    tone: (t) => tones[t ?? "plain"],
  };
}

export function palette(screen: string, caseName: string, bounces: number): Palette {
  const base =
    caseName === "rainbow" ? RAINBOW[bounces % RAINBOW.length]! : (CASE_COLOURS[caseName] ?? CASE_COLOURS.beige!);
  const darkCase = caseName === "graphite";
  return {
    room: ROOM,
    case: base,
    caseLight: mix(base, "#ffffff", 0.25),
    caseShade: mix(base, "#000000", 0.12),
    caseDark: darkCase ? mix(base, "#000000", 0.45) : mix(base, "#000000", 0.45),
    ...screenColours(screen),
  };
}
