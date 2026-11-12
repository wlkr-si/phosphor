import { Renderable, type CliRenderer, type MouseEvent, type OptimizedBuffer, type RenderContext } from "@opentui/core";
import { Bounce } from "../../src/bounce.ts";
import { rgb } from "../../src/color.ts";
import { drawComputer, geometry, type Rect } from "./computer.ts";
import { drawBoard, type Placed } from "./board.ts";
import { CASES, SCREENS, palette } from "./theme.ts";
import { Feeds } from "./sources.ts";
import type { Settings } from "./settings.ts";

/** True when `now` falls inside "HH:MM-HH:MM" (which may run past midnight). */
export function quiet(range: string, now: Date): boolean {
  const m = range.match(/^(\d{1,2}):(\d{2})\s*-\s*(\d{1,2}):(\d{2})$/);
  if (!m) return false;
  const [from, to] = [Number(m[1]) * 60 + Number(m[2]), Number(m[3]) * 60 + Number(m[4])];
  const t = now.getHours() * 60 + now.getMinutes();
  return from <= to ? t >= from && t < to : t >= from || t < to;
}

/** The screensaver: a beige computer drifting around the terminal, its screen showing cards. */
export class Screensaver extends Renderable {
  private bounce: Bounce;
  private bounces = 0;
  private feeds: Feeds;
  private placed: Placed[] = [];
  private computer: Rect = { x: 0, y: 0, w: 0, h: 0 };
  private zoomed: string | null = null;
  private paused = false;
  private notice: { text: string; until: number } | null = null;
  private drag: { dx: number; dy: number; moved: boolean; trail: { x: number; y: number; t: number }[] } | null = null;
  private timer: ReturnType<typeof setInterval>;
  private last = performance.now();
  private shown = "";

  constructor(
    ctx: RenderContext,
    private readonly renderer: CliRenderer,
    private readonly s: Settings,
    base: string,
  ) {
    super(ctx, { id: "screensaver", width: "100%", height: "100%" });
    this.bounce = new Bounce(s.speed);
    this.feeds = new Feeds(s.cards, base, () => this.requestRender());
    this.feeds.start();
    // Thirty times a second, move; draw only when something on screen would change.
    this.timer = setInterval(() => this.tick(), 1000 / 30);
    renderer.keyInput.on("keypress", (k) => this.key(k.name, k.sequence));
  }

  /** What a test or a caller can see: where the computer is and what the screen shows. */
  inspect(): { computer: Rect; zoomed: string | null; speed: number; paused: boolean; cards: Placed[] } {
    return { computer: { ...this.computer }, zoomed: this.zoomed, speed: this.bounce.speed, paused: this.paused, cards: this.placed };
  }

  stop(): void {
    clearInterval(this.timer);
    this.feeds.stop();
  }

  private size(): { w: number; h: number; roomW: number; roomH: number } {
    const W = this.renderer.width;
    const H = this.renderer.height;
    const w = Math.max(30, Math.round(W * this.s.size));
    const h = Math.max(16, Math.round(H * this.s.size));
    return { w, h, roomW: W - w, roomH: H - h };
  }

  private tick(): void {
    const now = performance.now();
    const dt = Math.min(0.25, (now - this.last) / 1000);
    this.last = now;
    if (!this.paused && !this.drag && !quiet(this.s.quietHours, new Date())) {
      const { roomW, roomH } = this.size();
      if (this.bounce.step(dt, roomW, roomH)) this.bounces++;
    }
    // Redraw when the computer lands on a new cell or the clock ticks.
    const key = `${Math.round(this.bounce.x)},${Math.round(this.bounce.y)},${Math.floor(Date.now() / 1000)}`;
    if (key !== this.shown) {
      this.shown = key;
      this.requestRender();
    }
  }

  protected renderSelf(buf: OptimizedBuffer): void {
    const now = new Date();
    const p = palette(this.s.screen, this.s.case, this.bounces);
    buf.fillRect(0, 0, buf.width, buf.height, rgb(p.room));
    if (quiet(this.s.quietHours, now)) {
      this.placed = [];
      return;
    }
    const { w, h, roomW, roomH } = this.size();
    const x = Math.round(Math.min(Math.max(this.bounce.x, 0), Math.max(roomW, 0)));
    const y = Math.round(Math.min(Math.max(this.bounce.y, 0), Math.max(roomH, 0)));
    this.computer = { x, y, w, h };
    const g = geometry(w, h);
    drawComputer(buf, x, y, g, p, this.s.label, now.getSeconds() % 2 === 0);
    const screen = { x: x + g.screen.x, y: y + g.screen.y, w: g.screen.w, h: g.screen.h };
    const notice = this.notice && this.notice.until > Date.now() ? this.notice.text : null;
    this.placed = drawBoard(buf, screen, this.s, p, (c) => this.feeds.get(c), now, this.zoomed, notice);
  }

  private say(text: string): void {
    this.notice = { text, until: Date.now() + 1500 };
    this.requestRender();
    setTimeout(() => this.requestRender(), 1600);
  }

  private key(name: string, seq: string): void {
    if (name === "space") {
      this.paused = !this.paused;
      this.say(this.paused ? "PAUSED" : "MOVING");
    } else if (name === "s") {
      this.s.screen = SCREENS[(SCREENS.indexOf(this.s.screen as never) + 1) % SCREENS.length]!;
      this.say(`SCREEN ${this.s.screen.toUpperCase()}`);
    } else if (name === "c") {
      this.s.case = CASES[(CASES.indexOf(this.s.case as never) + 1) % CASES.length]!;
      this.say(`CASE ${this.s.case.toUpperCase()}`);
    } else if (seq === "+" || seq === "=" || name === "up") this.speed(1.25);
    else if (seq === "-" || seq === "_" || name === "down") this.speed(0.8);
    else if (name === "escape" || name === "0") this.zoom(null);
    else if (/^[1-9]$/.test(seq)) this.zoom(this.s.cards[Number(seq) - 1]?.id ?? null);
  }

  private speed(by: number): void {
    const next = this.bounce.speed === 0 && by > 1 ? 0.5 : this.bounce.speed * by;
    this.bounce.setSpeed(next < 0.2 ? 0 : Math.min(next, 40));
    this.s.speed = this.bounce.speed;
    this.say(`SPEED ${this.bounce.speed.toFixed(1)}`);
  }

  private zoom(id: string | null): void {
    this.zoomed = id;
    this.requestRender();
  }

  private inside(e: MouseEvent, r: Rect): boolean {
    return e.x >= r.x && e.x < r.x + r.w && e.y >= r.y && e.y < r.y + r.h;
  }

  protected onMouseEvent(e: MouseEvent): void {
    if (e.type === "scroll" && e.scroll) {
      this.speed(e.scroll.direction === "up" ? 1.25 : 0.8);
    } else if (e.type === "down" && this.inside(e, this.computer)) {
      this.drag = { dx: e.x - this.computer.x, dy: e.y - this.computer.y, moved: false, trail: [{ x: e.x, y: e.y, t: performance.now() }] };
    } else if ((e.type === "drag" || e.type === "move") && this.drag) {
      const { roomW, roomH } = this.size();
      this.drag.moved ||= e.x !== this.drag.trail[0]!.x || e.y !== this.drag.trail[0]!.y;
      this.bounce.x = Math.min(Math.max(e.x - this.drag.dx, 0), Math.max(roomW, 0));
      this.bounce.y = Math.min(Math.max(e.y - this.drag.dy, 0), Math.max(roomH, 0));
      this.drag.trail.push({ x: e.x, y: e.y, t: performance.now() });
      if (this.drag.trail.length > 8) this.drag.trail.shift();
      this.requestRender();
    } else if ((e.type === "up" || e.type === "drag-end") && this.drag) {
      const d = this.drag;
      this.drag = null;
      this.last = performance.now();
      if (d.moved) {
        // Throw it with the speed of the last tenth of a second of dragging.
        const end = d.trail[d.trail.length - 1]!;
        const start = d.trail.find((s) => end.t - s.t <= 120) ?? d.trail[0]!;
        const secs = Math.max((end.t - start.t) / 1000, 1 / 60);
        this.bounce.fling((end.x - start.x) / secs, (end.y - start.y) / secs);
      } else {
        const hit = this.placed.find((pl) => this.inside(e, pl.rect));
        this.zoom(this.zoomed ? null : (hit?.card.id ?? null));
      }
    }
  }
}
