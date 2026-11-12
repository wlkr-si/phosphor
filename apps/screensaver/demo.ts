import type { CardType } from "./settings.ts";
import type { CardData, ChecksData, DotsData, ListData, MachineData, ProgressData, TextData, Tone } from "./data.ts";

/** Made-up data that moves, so the example looks alive before anyone wires up their own. */

function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return h >>> 0;
}

function rnd(seed: number): number {
  let x = Math.imul(seed, 2654435761) + 0x9e3779b9;
  x ^= x >>> 15;
  x = Math.imul(x, 0x85ebca6b);
  x ^= x >>> 13;
  return (x & 0xffff) / 65535;
}

/** A smooth wandering value in 0..1. */
export function wander(seed: number, x: number): number {
  const k = Math.floor(x);
  const a = rnd(seed + k);
  const b = rnd(seed + k + 1);
  const u = x - k;
  return a + (b - a) * u * u * (3 - 2 * u);
}

function cpuAt(seed: number, t: number): number {
  return Math.round(8 + 70 * (0.7 * wander(seed, t / 9) + 0.3 * wander(seed + 7, t / 2)) ** 1.6);
}

function ago(seconds: number): string {
  if (seconds < 60) return `${Math.max(1, Math.round(seconds))}s ago`;
  if (seconds < 3600) return `${Math.round(seconds / 60)}m ago`;
  return `${Math.round(seconds / 3600)}h ago`;
}

export function demo(type: CardType, name: string, t: number): CardData {
  const seed = hash(name);
  switch (type) {
    case "machine": {
      const history: number[] = [];
      for (let i = 199; i >= 0; i--) history.push(cpuAt(seed, t - i));
      return {
        cpu: cpuAt(seed, t),
        memory: { used: Math.round(10 + 6 * wander(seed + 3, t / 40)), total: 32 },
        disk: { used: 412, total: 1000 },
        facts: [
          { text: "8/8 checks ok", tone: "good" },
          { text: `${12 + Math.round(3 * wander(seed + 5, t / 60))} containers`, tone: "dim" },
        ],
        history,
      } satisfies MachineData;
    }
    case "list": {
      if (name.includes("deploy")) {
        const names = ["website", "api", "worker", "docs", "billing", "search", "mobile", "admin", "status"];
        const items: ListData["items"] = names.map((n, i) => {
          const age = 140 + i * 610 + (t % 600);
          const running = i === 0 && t % 90 < 30;
          const fail = rnd(seed + i) > 0.82;
          return {
            tag: running ? "run " : fail ? "fail" : "pass",
            tone: running ? "busy" : fail ? "bad" : "good",
            label: `example/${n}`,
            detail: fail ? "tests" : "build",
            right: ago(running ? t % 30 : age),
          };
        });
        return { items } satisfies ListData;
      }
      const names = ["web", "api", "postgres", "redis", "queue", "cron", "proxy", "metrics", "backups"];
      return {
        items: names.map((n, i) => {
          const down = i === 5 && t % 120 > 100;
          return { label: n, tone: down ? "bad" : "good", right: down ? "restarting" : "running" };
        }),
      } satisfies ListData;
    }
    case "progress":
      return {
        items: [
          { label: "launch the website", done: 9 + Math.floor((t / 60) % 4), total: 12 },
          { label: "move to the new server", done: 3, total: 8 },
          { label: "write the docs", done: 14, total: 14, tone: "good" },
        ],
        facts: [{ text: `Working now  ${t % 40 < 25 ? 2 : 1}`, tone: "busy" }],
      } satisfies ProgressData;
    case "checks": {
      const names = ["Disk space", "Docker", "Containers", "Firewall", "Updates", "Reboot", "Backups", "Build cache"];
      return {
        groups: ["server", "this-computer"].map((g, gi) => ({
          name: g,
          checks: names.map((n, i) => ({
            name: n,
            ok: !(gi === 0 && i === 4 && t % 300 > 200),
            detail: "12 updates waiting",
          })),
        })),
      } satisfies ChecksData;
    }
    case "dots": {
      const groups = ["example-org", "side-project"].map((g, gi) => {
        const units: Tone[] = [0, 1, 2].map((i) => (wander(seed + gi * 3 + i, t / 8) > 0.6 ? "busy" : "good"));
        const busy = units.filter((u) => u === "busy").length;
        return { name: g, units, note: `${units.length} online  ${busy} busy` };
      });
      return { groups } satisfies DotsData;
    }
    case "text":
      return { lines: ["Hello from Phosphor.", { text: "Point this card at your own data.", tone: "dim" }] } satisfies TextData;
  }
}
