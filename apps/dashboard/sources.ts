import { cpus, loadavg, totalmem, freemem, uptime, hostname } from "node:os";
import { readFileSync, statfsSync } from "node:fs";
import { resolve } from "node:path";
import type { CardSettings, Source } from "./settings.ts";
import type { CardData, MachineData } from "./data.ts";
import { demo } from "./demo.ts";

/** What a card has to draw right now. */
export interface Reading {
  data: CardData | null;
  /** Why there is no fresh data, if there is none. */
  error: string | null;
}

const HISTORY = 300;

/** One place data comes from, fetched on its own timer and shared by every card that uses it. */
class Feed {
  value: unknown = null;
  error: string | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private busy = false;

  constructor(
    readonly source: Source,
    private readonly base: string,
    private readonly onChange: () => void,
    private readonly system: SystemSampler,
  ) {}

  every(): number {
    const s = this.source;
    if ("demo" in s) return 1;
    const given = "every" in s && typeof s.every === "number" && s.every > 0 ? s.every : 0;
    if (given) return given;
    if ("system" in s) return 1;
    if ("file" in s) return 5;
    return 30;
  }

  start(): void {
    const tick = async () => {
      if (!this.busy) {
        this.busy = true;
        try {
          this.value = await this.read();
          this.error = null;
        } catch (e) {
          this.error = (e as Error).message || "could not read";
        }
        this.busy = false;
        this.onChange();
      }
      this.timer = setTimeout(tick, this.every() * 1000);
    };
    void tick();
  }

  stop(): void {
    if (this.timer) clearTimeout(this.timer);
  }

  private async read(): Promise<unknown> {
    const s = this.source;
    if ("demo" in s) return null; // drawn from the clock; see Feeds.get
    if ("system" in s) return this.system.sample();
    if ("file" in s) return parse(readFileSync(resolve(this.base, s.file), "utf8"));
    if ("url" in s) {
      const res = await fetch(s.url, { headers: s.headers, signal: AbortSignal.timeout(10_000) });
      if (!res.ok) throw new Error(`${res.status} from ${new URL(s.url).host}`);
      return parse(await res.text());
    }
    if ("command" in s) {
      const p = Bun.spawn(["sh", "-c", s.command], { cwd: this.base, stdout: "pipe", stderr: "ignore" });
      const killer = setTimeout(() => p.kill(), 20_000);
      const out = await new Response(p.stdout).text();
      clearTimeout(killer);
      if ((await p.exited) !== 0) throw new Error("the command failed");
      return parse(out);
    }
    throw new Error("unknown source");
  }
}

/** JSON if it is JSON, otherwise the text's lines. */
function parse(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return { lines: text.replace(/\s+$/, "").split("\n") };
  }
}

/** Follow a path like `machines[0].source.data` into a value. */
export function pick(value: unknown, path: string | undefined): unknown {
  if (!path) return value;
  let v: unknown = value;
  for (const part of path.match(/[^.[\]]+/g) ?? []) {
    if (v === null || typeof v !== "object") return undefined;
    v = (v as Record<string, unknown>)[part];
  }
  return v;
}

/** This computer's own numbers. */
class SystemSampler {
  private last = cpus().map((c) => c.times);

  sample(): MachineData {
    const now = cpus().map((c) => c.times);
    let busy = 0;
    let all = 0;
    now.forEach((t, i) => {
      const p = this.last[i] ?? t;
      const d = (k: keyof typeof t) => t[k] - p[k];
      const total = d("user") + d("nice") + d("sys") + d("idle") + d("irq");
      all += total;
      busy += total - d("idle");
    });
    this.last = now;
    const gb = (b: number) => Math.round((b / 1e9) * 10) / 10;
    let disk: MachineData["disk"];
    try {
      const st = statfsSync(process.platform === "darwin" ? "/System/Volumes/Data" : "/");
      disk = { used: gb((st.blocks - st.bfree) * st.bsize), total: gb(st.blocks * st.bsize) };
    } catch {
      disk = undefined;
    }
    const up = uptime();
    const days = Math.floor(up / 86400);
    const hours = Math.floor((up % 86400) / 3600);
    return {
      cpu: all > 0 ? Math.round((busy / all) * 100) : 0,
      memory: { used: gb(usedMemory()), total: gb(totalmem()) },
      disk,
      facts: [
        { text: hostname().replace(/\.local$/, ""), tone: "plain" },
        { text: `up ${days ? `${days}d ` : ""}${hours}h`, tone: "dim" },
        { text: `load ${loadavg()[0]!.toFixed(2)}`, tone: "dim" },
      ],
    };
  }
}

/** Memory in use, not counting what the system is only borrowing as cache. */
function usedMemory(): number {
  if (process.platform === "linux") {
    try {
      const info = readFileSync("/proc/meminfo", "utf8");
      const kb = (k: string) => Number(info.match(new RegExp(`^${k}:\\s+(\\d+)`, "m"))?.[1] ?? 0) * 1024;
      return kb("MemTotal") - kb("MemAvailable");
    } catch {
      /* fall through */
    }
  }
  if (process.platform === "darwin") {
    try {
      const out = Bun.spawnSync(["vm_stat"]).stdout.toString();
      const page = Number(out.match(/page size of (\d+)/)?.[1] ?? 16384);
      const pages = (k: string) => Number(out.match(new RegExp(`${k}:\\s+(\\d+)`))?.[1] ?? 0);
      return (pages("Pages active") + pages("Pages wired down") + pages("Pages occupied by compressor")) * page;
    } catch {
      /* fall through */
    }
  }
  return totalmem() - freemem();
}

/** Every card's data, kept fresh. */
export class Feeds {
  private feeds = new Map<string, Feed>();
  private history = new Map<string, number[]>();
  private system = new SystemSampler();
  private started = Date.now();

  constructor(
    private readonly cards: CardSettings[],
    base: string,
    onChange: () => void,
  ) {
    for (const c of cards) {
      const key = feedKey(c.source);
      if (!this.feeds.has(key))
        this.feeds.set(
          key,
          new Feed(
            c.source,
            base,
            () => {
              this.record(key);
              onChange();
            },
            this.system,
          ),
        );
    }
  }

  start(): void {
    for (const f of this.feeds.values()) f.start();
  }

  stop(): void {
    for (const f of this.feeds.values()) f.stop();
  }

  get(card: CardSettings): Reading {
    const s = card.source;
    if ("demo" in s) return { data: demo(card.type, s.demo || card.id, (Date.now() - this.started) / 1000 + 600), error: null };
    const f = this.feeds.get(feedKey(s));
    if (!f) return { data: null, error: "no source" };
    const v = pick(f.value, "path" in s ? s.path : undefined);
    if (v === undefined || v === null) return { data: null, error: f.error ?? (f.value === null ? null : "nothing at that path") };
    let data = v as CardData;
    if (card.type === "machine") {
      const m = data as MachineData;
      if (!m.history) data = { ...m, history: this.history.get(card.id) ?? [] };
    }
    return { data, error: f.error };
  }

  /** Keep a processor history for machine cards whose data does not bring its own. */
  private record(key: string): void {
    for (const c of this.cards) {
      if (c.type !== "machine" || "demo" in c.source || feedKey(c.source) !== key) continue;
      const f = this.feeds.get(feedKey(c.source));
      const v = pick(f?.value, "path" in c.source ? c.source.path : undefined) as MachineData | undefined;
      if (!v || typeof v.cpu !== "number" || v.history) continue;
      const h = this.history.get(c.id) ?? [];
      h.push(v.cpu);
      if (h.length > HISTORY) h.splice(0, h.length - HISTORY);
      this.history.set(c.id, h);
    }
  }
}

function feedKey(s: Source): string {
  const { path: _path, ...rest } = s as Source & { path?: string };
  return JSON.stringify(rest);
}
