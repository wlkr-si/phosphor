import { OptimizedBuffer, Renderable, type CliRenderer, type MouseEvent, type RenderContext } from "@opentui/core";
import { Bounce } from "../../src/bounce.ts";
import { rgb } from "../../src/color.ts";
import { Art, blockSteps, drawChin, geometry, placement, type Rect } from "./computer.ts";
import { drawBoard, type Placed } from "./board.ts";
import { CASES, SCREENS, palette } from "./theme.ts";
import { Feeds } from "./sources.ts";
import { Anim } from "./anim.ts";
import type { Settings } from "./settings.ts";

/** True when `now` falls inside "HH:MM-HH:MM" (which may run past midnight). */
export function quiet(range: string, now: Date): boolean {
  const m = range.match(/^(\d{1,2}):(\d{2})\s*-\s*(\d{1,2}):(\d{2})$/);
  if (!m) return false;
  const [from, to] = [Number(m[1]) * 60 + Number(m[2]), Number(m[3]) * 60 + Number(m[4])];
  const t = now.getHours() * 60 + now.getMinutes();
  return from <= to ? t >= from && t < to : t >= from || t < to;
}

/** Frames a second while moving. Terminals that paint slower simply show fewer of them. */
const FPS = 30;
/** In glide mode: how long it rests, how long a glide takes, and how far it goes per unit of speed. */
const REST_MS = 9000;
const GLIDE_MS = 1700;
const GLIDE_CELLS = 8;

/** The screensaver: a beige computer drifting around the terminal, its screen showing cards. */
export class Screensaver extends Renderable {
  private bounce: Bounce;
  private bounces = 0;
  private feeds: Feeds;
  private art: Art;
  private anim = new Anim();
  /** The screen's contents, drawn only when they change and copied into place each frame. */
  private board: OptimizedBuffer | null = null;
  private dirty = true;
  private boardSecond = -1;
  private placed: Placed[] = [];
  private computer: Rect = { x: 0, y: 0, w: 0, h: 0 };
  private zoomed: string | null = null;
  private paused = false;
  private notice: { text: string; until: number } | null = null;
  private drag: { dx: number; dy: number; moved: boolean; trail: { x: number; y: number; t: number }[] } | null = null;
  private timer: ReturnType<typeof setInterval>;
  private last = performance.now();
  private shown = "";
  private steps = blockSteps();
  /** When the next glide starts. */
  private glideAt = performance.now() + 2500;

  constructor(
    ctx: RenderContext,
    private readonly renderer: CliRenderer,
    private readonly s: Settings,
    base: string,
  ) {
    super(ctx, { id: "screensaver", width: "100%", height: "100%" });
    this.bounce = new Bounce(s.speed);
    this.art = new Art(renderer, this.steps);
    this.feeds = new Feeds(s.cards, base, () => this.invalidate());
    this.feeds.start();
    this.timer = setInterval(() => this.tick(), 1000 / FPS);
    renderer.keyInput.on("keypress", (k) => this.key(k.name, k.sequence));
  }

  /** What a test or a caller can see: where the computer is and what the screen shows. */
  inspect(): { computer: Rect; zoomed: string | null; speed: number; paused: boolean; cards: Placed[] } {
    const cards = this.placed.map((pl) => ({ ...pl, rect: { ...pl.rect, x: pl.rect.x + this.boardAt.x, y: pl.rect.y + this.boardAt.y } }));
    return { computer: { ...this.computer }, zoomed: this.zoomed, speed: this.bounce.speed, paused: this.paused, cards };
  }

  stop(): void {
    clearInterval(this.timer);
    this.feeds.stop();
    this.art.clear();
    this.board?.destroy();
    this.board = null;
  }

  /** The screen's contents have changed: draw them again on the next frame. */
  private invalidate(): void {
    this.dirty = true;
    this.requestRender();
  }

  private size(): { w: number; h: number; roomW: number; roomH: number } {
    const W = this.renderer.width;
    const H = this.renderer.height;
    const w = Math.min(W, Math.max(30, Math.round(W * this.s.size)));
    const h = Math.min(H, Math.max(16, Math.round(H * this.s.size)));
    return { w, h, roomW: W - w, roomH: H - h };
  }

  private tick(): void {
    const now = performance.now();
    const dt = Math.min(0.25, (now - this.last) / 1000);
    this.last = now;
    const asleep = quiet(this.s.quietHours, new Date());
    if (!this.paused && !this.drag && !asleep) {
      const { roomW, roomH } = this.size();
      if (this.bounce.step(dt, roomW, roomH, this.pace(now))) this.bounces++;
    }
    // Draw when the computer has moved an eighth of a cell, the clock ticks, or something is easing.
    const pl = placement(this.bounce.x, this.bounce.y, this.steps);
    const key = `${pl.cellX},${pl.qx},${pl.cellY},${pl.qy},${Math.floor(Date.now() / 1000)},${asleep}`;
    if (key !== this.shown || this.anim.busy || this.dirty) {
      this.shown = key;
      this.requestRender();
    } else if (!asleep) {
      // Nothing to draw this frame: spend it drawing a case position we have not needed yet.
      const { w, h } = this.size();
      this.art.warm(geometry(w, h), palette(this.s.screen, this.s.case, this.bounces), 1);
    }
  }

  /**
   * How fast to move now, as a share of the drifting speed. Text can only sit on whole
   * characters, so slow constant motion shows as steps; gliding quickly between rests reads
   * as motion and keeps the screen still for reading most of the time.
   */
  private pace(now: number): number {
    if (this.s.motion === "drift") return 1;
    const u = (now - this.glideAt) / GLIDE_MS;
    if (u < 0) return 0;
    if (u >= 1) {
      this.glideAt = now + REST_MS;
      return 0;
    }
    // A sine-shaped speed: starts and ends gently, and covers GLIDE_CELLS x speed in all.
    const seconds = GLIDE_MS / 1000;
    return ((GLIDE_CELLS * Math.PI) / (2 * seconds)) * Math.sin(Math.PI * u);
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
    const g = geometry(w, h);
    const pl = placement(
      Math.min(Math.max(this.bounce.x, 0), Math.max(roomW, 0)),
      Math.min(Math.max(this.bounce.y, 0), Math.max(roomH, 0)),
      this.steps,
    );
    this.computer = { x: pl.snapX, y: pl.snapY, w, h };
    buf.drawFrameBuffer(pl.cellX, pl.cellY, this.art.picture(g, p, pl.qx, pl.qy));
    drawChin(buf, pl.snapX, pl.snapY, g, p, this.s.label, now.getSeconds() % 2 === 0);

    // Redraw the screen's contents only when they change; otherwise copy the last drawing.
    const sw = g.screen.w;
    const sh = g.screen.h;
    if (!this.board || this.board.width !== sw || this.board.height !== sh) {
      this.board?.destroy();
      this.board = OptimizedBuffer.create(sw, sh, this.renderer.widthMethod, { owner: this.renderer.nativeScene });
      this.dirty = true;
    }
    const second = Math.floor(now.getTime() / 1000);
    if (this.dirty || this.anim.busy || second !== this.boardSecond) {
      this.dirty = false;
      this.boardSecond = second;
      this.anim.frame(performance.now());
      const notice = this.notice && this.notice.until > Date.now() ? this.notice.text : null;
      this.placed = drawBoard(this.board, { x: 0, y: 0, w: sw, h: sh }, this.s, p, (c) => this.feeds.get(c), now, this.zoomed, notice, this.anim);
    }
    const sx = pl.snapX + g.screen.x;
    const sy = pl.snapY + g.screen.y;
    buf.drawFrameBuffer(sx, sy, this.board);
    this.boardAt = { x: sx, y: sy };
  }

  /** Where the screen's contents were last copied to, for finding the card under the mouse. */
  private boardAt = { x: 0, y: 0 };

  private say(text: string): void {
    this.notice = { text, until: Date.now() + 1500 };
    this.invalidate();
    setTimeout(() => this.invalidate(), 1600);
  }

  private key(name: string, seq: string): void {
    if (name === "space") {
      this.paused = !this.paused;
      this.say(this.paused ? "PAUSED" : "MOVING");
    } else if (name === "s") {
      this.s.screen = SCREENS[(SCREENS.indexOf(this.s.screen as never) + 1) % SCREENS.length]!;
      this.say(`SCREEN ${this.s.screen.toUpperCase()}`);
    } else if (name === "m") {
      this.s.motion = this.s.motion === "drift" ? "glide" : "drift";
      this.glideAt = performance.now() + 600;
      this.say(this.s.motion.toUpperCase());
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
    this.invalidate();
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
        const hit = this.inspect().cards.find((pl) => this.inside(e, pl.rect));
        this.zoom(this.zoomed ? null : (hit?.card.id ?? null));
      }
    }
  }
}
