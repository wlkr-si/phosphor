/**
 * The data each card type draws. A source must produce one of these shapes (as JSON).
 * Every field is optional unless marked; a card shows what it is given.
 */

/** How something is doing. Each tone has its own colour. */
export type Tone = "good" | "bad" | "busy" | "warn" | "dim" | "plain";

/** A short piece of text with a tone. */
export interface Fact {
  text: string;
  tone?: Tone;
}

/** Used and total, in any one unit (GB by default). */
export interface Amount {
  used: number;
  total: number;
  unit?: string;
}

/** A computer: processor, memory and disk bars, a line of facts and a processor chart. */
export interface MachineData {
  /** Processor use, 0 to 100. Also feeds the chart when `history` is not given. */
  cpu?: number;
  memory?: Amount;
  disk?: Amount;
  facts?: Fact[];
  /** Processor use over time, oldest first, 0 to 100. */
  history?: number[];
  /** True when the machine could not be reached. */
  unreachable?: boolean;
}

/** Rows of things: a coloured tag or marker, a label, a dim detail and a right-hand note. */
export interface ListData {
  items: { label: string; tag?: string; tone?: Tone; detail?: string; right?: string }[];
}

/** Progress bars, such as tasks done out of total, then a line of facts and any notes. */
export interface ProgressData {
  items: { label: string; done: number; total: number; tone?: Tone }[];
  facts?: Fact[];
  lines?: (string | Fact)[];
}

/** Groups of checks that pass or fail. Failures are listed first, in red. */
export interface ChecksData {
  groups: { name: string; checks: { name: string; ok: boolean; detail?: string }[] }[];
}

/** Groups of small blocks, one per worker, coloured by state. */
export interface DotsData {
  groups: { name: string; units: Tone[]; note?: string }[];
}

/** Plain lines of text. */
export interface TextData {
  lines: (string | Fact)[];
}

type Content = MachineData | ListData | ProgressData | ChecksData | DotsData | TextData;

/** Optional content shown only when a card fills the screen. */
export type CardData = Content & {
  expanded?: { type: "machine" | "list" | "progress" | "checks" | "dots" | "text"; data: Content };
};
