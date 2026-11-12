import { createCliRenderer, type CliRenderer } from "@opentui/core";

/** What every Phosphor app gets from the command line. */
export interface Launch {
  /** Quit on its own after this many seconds (for tests and screenshots). */
  seconds?: number;
}

/**
 * Take over the whole terminal for an app, and give it back cleanly when the app quits,
 * when q or Ctrl-C is pressed, or when `seconds` runs out.
 * `start` builds the app and returns a function that stops its timers.
 */
export async function launch(
  launch: Launch,
  start: (renderer: CliRenderer, quit: () => void) => (() => void) | void,
): Promise<void> {
  const renderer = await createCliRenderer({
    exitOnCtrlC: false,
    useMouse: true,
    enableMouseMovement: true,
    targetFps: 30,
    maxFps: 30,
    screenMode: "alternate-screen",
    consoleMode: "disabled",
    backgroundColor: "#000000",
  });
  let stopped = false;
  let stopApp: (() => void) | void = undefined;
  const quit = () => {
    if (stopped) return;
    stopped = true;
    stopApp?.();
    renderer.destroy();
  };
  renderer.keyInput.on("keypress", (k) => {
    if (k.name === "q" || (k.ctrl && k.name === "c")) quit();
  });
  for (const sig of ["SIGTERM", "SIGHUP", "SIGINT"] as const) process.on(sig, quit);
  stopApp = start(renderer, quit);
  if (launch.seconds && launch.seconds > 0) setTimeout(quit, launch.seconds * 1000);
  await new Promise<void>((done) => renderer.once("destroy", () => done()));
  await renderer.closed;
}

/** Read `--name value` and `--flag` options. */
export function options(argv: string[]): Map<string, string> {
  const out = new Map<string, string>();
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (!a.startsWith("--")) continue;
    const next = argv[i + 1];
    if (next !== undefined && !next.startsWith("--")) {
      out.set(a.slice(2), next);
      i++;
    } else out.set(a.slice(2), "");
  }
  return out;
}
