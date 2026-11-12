// Build every app into one program file per computer type, in dist/.
// usage: bun scripts/build.ts [app ...] [--target linux-x64,darwin-arm64]
// Needs the native parts for every target: run `bun install --os='*' --cpu='*'` first.
import { readdirSync, existsSync, rmSync, mkdirSync } from "node:fs";
import { join } from "node:path";

const ALL = ["darwin-arm64", "darwin-x64", "linux-x64", "linux-arm64"];
const args = process.argv.slice(2);
const t = args.indexOf("--target");
const targets = t >= 0 ? args[t + 1]!.split(",") : ALL;
const apps = args.filter((a, i) => !a.startsWith("--") && i !== t + 1);
const root = join(import.meta.dir, "..");
const names = apps.length ? apps : readdirSync(join(root, "apps")).filter((a) => existsSync(join(root, "apps", a, "main.ts")));

rmSync(join(root, "dist"), { recursive: true, force: true });
mkdirSync(join(root, "dist"));
for (const app of names) {
  for (const target of targets) {
    const out = join(root, "dist", `phosphor-${app}-${target}`);
    const p = Bun.spawnSync(
      ["bun", "build", join(root, "apps", app, "main.ts"), "--compile", "--minify", `--target=bun-${target}`, "--outfile", out],
      { stdout: "inherit", stderr: "inherit" },
    );
    if (p.exitCode !== 0) process.exit(p.exitCode ?? 1);
  }
}
