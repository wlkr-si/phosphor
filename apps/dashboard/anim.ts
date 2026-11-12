/**
 * Changes on the screen ease in instead of jumping: numbers and bars slide to their new
 * values, and rows that are new glow for a moment and fade.
 */

const SLIDE_MS = 900;
/** Values that change more often than this slide only briefly, so each tick still reads. */
const QUICK_GAP_MS = 1500;
const QUICK_SLIDE_MS = 200;
const GLOW_MS = 2600;

const ease = (t: number) => 1 - (1 - t) ** 3;

interface Slide {
  from: number;
  to: number;
  at: number;
  ms: number;
}

export class Anim {
  private slides = new Map<string, Slide>();
  private seen = new Map<string, Set<string>>();
  /** The frame each group was first drawn in: everything in that first drawing is not new. */
  private born = new Map<string, number>();
  private frames = 0;
  private glows = new Map<string, number>();
  private now = 0;
  /** True while something is still moving, so the screen keeps being redrawn. */
  busy = false;

  /** Call once before drawing a frame. */
  frame(now: number): void {
    this.now = now;
    this.busy = false;
    this.frames++;
  }

  /** The value to show for `key` now, sliding toward `target` whenever it changes. */
  value(key: string, target: number): number {
    const s = this.slides.get(key);
    if (!s) {
      this.slides.set(key, { from: target, to: target, at: 0, ms: SLIDE_MS });
      return target;
    }
    if (s.to !== target) {
      s.from = this.current(s);
      s.to = target;
      s.ms = s.at && this.now - s.at < QUICK_GAP_MS ? QUICK_SLIDE_MS : SLIDE_MS;
      s.at = this.now;
    }
    const v = this.current(s);
    if (v !== s.to) this.busy = true;
    return v;
  }

  private current(s: Slide): number {
    const t = Math.min(1, (this.now - s.at) / s.ms);
    return t >= 1 ? s.to : s.from + (s.to - s.from) * ease(t);
  }

  /**
   * How brightly a row should glow, 0 to 1. A row glows when it appears in a group (such as
   * a card's list) after the group was first shown, or when its text changes.
   */
  glow(group: string, row: string): number {
    let seen = this.seen.get(group);
    if (!seen) {
      this.seen.set(group, (seen = new Set()));
      this.born.set(group, this.frames);
    }
    if (!seen.has(row)) {
      seen.add(row);
      if (this.born.get(group) !== this.frames) this.glows.set(`${group}\n${row}`, this.now);
      if (seen.size > 2000) seen.clear();
    }
    const at = this.glows.get(`${group}\n${row}`);
    if (at === undefined) return 0;
    const t = (this.now - at) / GLOW_MS;
    if (t >= 1) {
      this.glows.delete(`${group}\n${row}`);
      return 0;
    }
    this.busy = true;
    return 1 - ease(t);
  }
}
