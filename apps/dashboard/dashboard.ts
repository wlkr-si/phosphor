import { OptimizedBuffer, Renderable, type CliRenderer, type MouseEvent, type RenderContext } from "@opentui/core";
import { rgb } from "../../src/color.ts";
import { Saver } from "../../src/saver.ts";
import { artPixels } from "../../src/art.ts";
import { Art, drawChin, geometry, type Rect } from "./computer.ts";
import { cardContent } from "./cards.ts";
import { drawBoard, type Placed } from "./board.ts";
import { CASES, SCREENS, palette, type Palette } from "./theme.ts";
import { Feeds } from "./sources.ts";
import { Anim } from "./anim.ts";
import type { Pictures, Settings } from "./settings.ts";

/** True when `now` falls inside "HH:MM-HH:MM" (which may run past midnight). */
export function quiet(range: string, now: Date): boolean {
  const m = range.match(/^(\d{1,2}):(\d{2})\s*-\s*(\d{1,2}):(\d{2})$/);
  if (!m) return false;
  const [from, to] = [Number(m[1]) * 60 + Number(m[2]), Number(m[3]) * 60 + Number(m[4])];
  const t = now.getHours() * 60 + now.getMinutes();
  return from <= to ? t >= from && t < to : t >= from || t < to;
}

/** Frames a second while the screensaver or a wipe is running. */
const FPS = 30;
/** How long the sweep between the dashboard and the screensaver takes. */
const WIPE_MS = 450;

type Mode = "board" | "to-saver" | "saver" | "to-board";

/**
 * The dashboard: a still retro computer filling the window, its screen showing cards. After
 * a while with no mouse or keys it hands the whole window to a screensaver; any mouse
 * movement, click or key brings the dashboard straight back and starts the wait again.
 */
export class Dashboard extends Renderable {
  private feeds: Feeds;
  private art: Art;
  private anim = new Anim();
  private saver: Saver;
  /** The screen's contents, drawn only when they change and copied into place each frame. */
  private board: OptimizedBuffer | null = null;
  private dirty = true;
  private boardSecond = -1;
  private placed: Placed[] = [];
  private boardAt = { x: 0, y: 0 };
  private zoomed: string | null = null;
  private offset = 0;
  private notice: { text: string; until: number } | null = null;
  private mode: Mode = "board";
  private modeAt = performance.now();
  private lastInput = performance.now();
  private timer: ReturnType<typeof setInterval>;
  private shown = "";

  constructor(
    ctx: RenderContext,
    private readonly renderer: CliRenderer,
    private readonly s: Settings,
    base: string,
    private readonly pictures: Pictures = { logo: null, saver: [] },
  ) {
    super(ctx, { id: "dashboard", width: "100%", height: "100%" });
    this.art = new Art(renderer);
    this.saver = new Saver(pictures.saver.length ? pictures.saver.map(artPixels) : s.saverText || s.label, s.effects);
    this.feeds = new Feeds(s.cards, base, () => this.invalidate());
    this.feeds.start();
    this.timer = setInterval(() => this.tick(), 1000 / FPS);
    renderer.keyInput.on("keypress", (k) => this.key(k.name, k.sequence));
  }

  /** What a test or a caller can see. */
  inspect(): { mode: Mode; zoomed: string | null; cards: Placed[] } {
    const cards = this.placed.map((pl) => ({ ...pl, rect: { ...pl.rect, x: pl.rect.x + this.boardAt.x, y: pl.rect.y + this.boardAt.y } }));
    return { mode: this.mode, zoomed: this.zoomed, cards };
  }

  stop(): void {
    clearInterval(this.timer);
    this.feeds.stop();
    this.art.clear();
    this.board?.destroy();
    this.board = null;
  }

  /** Show the screensaver now, as if the dashboard had been left alone. */
  sleep(): void {
    if (this.mode === "board" || this.mode === "to-board") this.go("to-saver");
  }

  /** Someone is here: bring the dashboard back and start the wait again. */
  wake(): void {
    this.lastInput = performance.now();
    if (this.mode === "saver" || this.mode === "to-saver") this.go("to-board");
  }

  private go(mode: Mode): void {
    this.mode = mode;
    this.modeAt = performance.now();
    if (mode === "saver") this.saver.start(this.modeAt);
    this.invalidate();
  }

  private invalidate(): void {
    this.dirty = true;
    this.requestRender();
  }

  private tick(): void {
    const now = performance.now();
    const since = now - this.modeAt;
    const asleep = quiet(this.s.quietHours, new Date());
    if (this.mode === "board" && this.s.screensaver > 0 && !asleep && now - this.lastInput >= this.s.idle * 1000) this.go("to-saver");
    else if (this.mode === "to-saver" && since >= WIPE_MS) this.go("saver");
    else if (this.mode === "saver" && since >= this.s.screensaver * 1000) {
      this.lastInput = now;
      this.go("to-board");
    } else if (this.mode === "to-board" && since >= WIPE_MS) this.go("board");

    // While the dashboard shows, draw only when something on it changes.
    const moving = this.mode !== "board" || this.anim.busy || this.dirty;
    const key = `${Math.floor(Date.now() / 1000)},${asleep}`;
    if (moving || key !== this.shown) {
      this.shown = key;
      this.requestRender();
    }
  }

  private size(): { x: number; y: number; w: number; h: number } {
    const W = this.renderer.width;
    const H = this.renderer.height;
    const w = Math.min(W, Math.max(30, Math.round(W * this.s.size)));
    const h = Math.min(H, Math.max(16, Math.round(H * this.s.size)));
    return { x: Math.floor((W - w) / 2), y: Math.floor((H - h) / 2), w, h };
  }

  private colours(p: Palette) {
    return { bg: p.room, ink: p.ink, glow: "#eafff0", dim: p.dim, accents: [p.accent("cyan"), p.accent("magenta"), p.accent("amber")] };
  }

  protected renderSelf(buf: OptimizedBuffer): void {
    const now = new Date();
    const t = performance.now();
    const p = palette(this.s.screen, this.s.case);
    if (quiet(this.s.quietHours, now)) {
      buf.fillRect(0, 0, buf.width, buf.height, rgb("#000000"));
      this.placed = [];
      return;
    }
    if (this.mode === "saver") {
      this.saver.draw(buf, t, this.colours(p), now.toTimeString().slice(0, 5));
      return;
    }
    this.drawBoard(buf, p, now);
    if (this.mode === "to-saver" || this.mode === "to-board") {
      // A bright line sweeps across; behind it is dark (going to sleep) or the dashboard (waking).
      const u = Math.min(1, (t - this.modeAt) / WIPE_MS);
      const edge = Math.round(u * u * (3 - 2 * u) * (buf.width + 2));
      const dark = rgb(p.room);
      if (this.mode === "to-saver") buf.fillRect(0, 0, Math.max(0, edge - 1), buf.height, dark);
      else buf.fillRect(Math.max(0, edge + 1), 0, Math.max(0, buf.width - edge - 1), buf.height, dark);
      const line = rgb(p.ink);
      if (edge - 1 >= 0 && edge - 1 < buf.width) buf.fillRect(edge - 1, 0, 1, buf.height, rgb("#eafff0"));
      if (edge >= 0 && edge < buf.width) buf.fillRect(edge, 0, 1, buf.height, line);
    }
  }

  private drawBoard(buf: OptimizedBuffer, p: Palette, now: Date): void {
    buf.fillRect(0, 0, buf.width, buf.height, rgb(p.room));
    const at = this.size();
    const g = geometry(at.w, at.h);
    buf.drawFrameBuffer(at.x, at.y, this.art.picture(g, p));
    drawChin(buf, at.x, at.y, g, p, this.s.label, now.getSeconds() % 2 === 0);

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
      this.placed = drawBoard(this.board, { x: 0, y: 0, w: sw, h: sh }, this.s, p, (c) => this.feeds.get(c), now, this.zoomed, notice, this.anim, this.pictures.logo, this.offset);
    }
    this.boardAt = { x: at.x + g.screen.x, y: at.y + g.screen.y };
    buf.drawFrameBuffer(this.boardAt.x, this.boardAt.y, this.board);
  }

  private say(text: string): void {
    this.notice = { text, until: Date.now() + 1500 };
    this.invalidate();
    setTimeout(() => this.invalidate(), 1600);
  }

  private key(name: string, seq: string): void {
    const sleeping = this.mode !== "board";
    this.wake();
    if (sleeping) return; // A key that wakes the dashboard does nothing else.
    if (this.zoomed && ["up", "down", "pageup", "pagedown", "home", "end"].includes(name)) {
      this.scroll(name === "home" ? -Infinity : name === "end" ? Infinity : (name === "up" || name === "pageup" ? -1 : 1) * (name.startsWith("page") ? this.pageSize() : 1));
    } else if (name === "space") this.sleep();
    else if (name === "s") {
      this.s.screen = SCREENS[(SCREENS.indexOf(this.s.screen as never) + 1) % SCREENS.length]!;
      this.say(`SCREEN ${this.s.screen.toUpperCase()}`);
    } else if (name === "c") {
      this.s.case = CASES[(CASES.indexOf(this.s.case as never) + 1) % CASES.length]!;
      this.say(`CASE ${this.s.case.toUpperCase()}`);
    } else if (name === "escape" || name === "0") this.zoom(null);
    else if (/^[1-9]$/.test(seq)) this.zoom(this.s.cards[Number(seq) - 1]?.id ?? null);
  }

  private zoom(id: string | null): void {
    this.zoomed = id;
    this.offset = 0;
    this.invalidate();
  }

  private pageSize(): number {
    return Math.max(1, (this.placed[0]?.rect.h ?? 3) - 2);
  }

  private scroll(delta: number): void {
    const card = this.s.cards.find((c) => c.id === this.zoomed);
    if (!card) return;
    const { rows } = cardContent(card, this.feeds.get(card), true, Math.max(1, (this.placed[0]?.rect.w ?? 8) - 4));
    this.offset = Math.max(0, Math.min(Math.max(0, (rows?.length ?? 0) - this.pageSize()), this.offset + delta));
    this.invalidate();
  }

  private inside(e: MouseEvent, r: Rect): boolean {
    return e.x >= r.x && e.x < r.x + r.w && e.y >= r.y && e.y < r.y + r.h;
  }

  private pointer = { x: -1, y: -1 };

  protected onMouseEvent(e: MouseEvent): void {
    // Only real use counts: a move to a new cell, a click, a drag or a scroll. The terminal
    // also reports the pointer simply being over the window, which is not someone using it.
    if (e.type === "over" || e.type === "out") return;
    if (e.type === "move") {
      if (e.x === this.pointer.x && e.y === this.pointer.y) return;
      const first = this.pointer.x < 0;
      this.pointer = { x: e.x, y: e.y };
      if (first) return;
    }
    const sleeping = this.mode !== "board";
    this.wake();
    if (sleeping) return;
    if (e.type === "scroll" && this.zoomed) {
      this.scroll(e.scroll?.direction === "up" ? -3 : e.scroll?.direction === "down" ? 3 : 0);
      return;
    }
    if (e.type !== "up") return;
    const hit = this.inspect().cards.find((pl) => this.inside(e, pl.rect));
    this.zoom(this.zoomed ? null : (hit?.card.id ?? null));
  }
}
