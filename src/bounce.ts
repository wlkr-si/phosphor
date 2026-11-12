/**
 * Something that drifts around a room and bounces off its walls, like an old DVD logo.
 * Positions are in terminal cells. A cell is about twice as tall as it is wide, so the
 * default path moves two columns for every row to look like a true diagonal.
 */
export class Bounce {
  x = 0;
  y = 0;
  vx: number;
  vy: number;
  /** How fast it drifts, in cells per second, counting a row as two columns. */
  speed: number;
  private flinging = false;

  constructor(speed: number) {
    this.speed = speed;
    this.vx = 1;
    this.vy = 0.5;
    this.aim();
  }

  /** True while a throw is still slowing down. */
  get throwing(): boolean {
    return this.flinging;
  }

  /**
   * Advance `dt` seconds inside a room with `roomW` x `roomH` cells of free space, at `pace`
   * times the drifting speed (a throw always goes at its own speed). Returns true on a bounce.
   */
  step(dt: number, roomW: number, roomH: number, pace = 1): boolean {
    let bounced = false;
    const k = this.flinging ? 1 : pace;
    this.x += this.vx * dt * k;
    this.y += this.vy * dt * k;
    if (this.x < 0) [this.x, this.vx, bounced] = [-this.x, Math.abs(this.vx), true];
    if (this.x > roomW) [this.x, this.vx, bounced] = [Math.max(0, 2 * roomW - this.x), -Math.abs(this.vx), true];
    if (this.y < 0) [this.y, this.vy, bounced] = [-this.y, Math.abs(this.vy), true];
    if (this.y > roomH) [this.y, this.vy, bounced] = [Math.max(0, 2 * roomH - this.y), -Math.abs(this.vy), true];
    this.x = Math.min(Math.max(this.x, 0), Math.max(roomW, 0));
    this.y = Math.min(Math.max(this.y, 0), Math.max(roomH, 0));
    if (this.flinging) this.settle(dt);
    return bounced;
  }

  /** Throw it: it flies off at this velocity (cells per second) and eases back to drifting. */
  fling(vx: number, vy: number): void {
    const max = 120;
    this.vx = Math.max(-max, Math.min(max, vx));
    this.vy = Math.max(-max / 2, Math.min(max / 2, vy));
    this.flinging = true;
  }

  /** Change the drifting speed, keeping the direction. */
  setSpeed(speed: number): void {
    this.speed = Math.max(0, speed);
    if (!this.flinging) this.aim();
  }

  /** Ease a throw back down to the drifting speed, keeping its direction. */
  private settle(dt: number): void {
    const now = Math.hypot(this.vx, this.vy * 2);
    if (now <= this.speed * 1.02 || now === 0) {
      this.flinging = false;
      this.aim();
      return;
    }
    const next = Math.max(this.speed, now * Math.exp(-1.6 * dt));
    this.vx *= next / now;
    this.vy *= next / now;
  }

  /** Point the velocity along the current direction at the drifting speed. */
  private aim(): void {
    const sx = this.vx < 0 ? -1 : 1;
    const sy = this.vy < 0 ? -1 : 1;
    const now = Math.hypot(this.vx, this.vy * 2);
    if (now === 0) [this.vx, this.vy] = [(sx * this.speed) / Math.SQRT2, (sy * this.speed) / 2 / Math.SQRT2];
    else [this.vx, this.vy] = [(this.vx / now) * this.speed, (this.vy / now) * this.speed];
  }
}
