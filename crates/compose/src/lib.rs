//! Composition: what should be seen, and how it changes. Knows nothing about surfaces.
//! Words: canvas, layer, placement, motion. See CONTEXT.md.
//!
//! A scene is layers, back to front. Moving a layer is placing it somewhere else; its
//! canvas is never redrawn.

use phosphor_kernel::{Canvas, Rgb};

/// Where a layer sits, in dots: the position of its top-left dot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub x: i64,
    pub y: i64,
}

/// An area of the scene, in dots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

impl Rect {
    fn right(&self) -> i64 {
        self.x + self.w
    }
    fn bottom(&self) -> i64 {
        self.y + self.h
    }
    fn union(self, other: Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect {
            x,
            y,
            w: self.right().max(other.right()) - x,
            h: self.bottom().max(other.bottom()) - y,
        }
    }
    /// The part of this rectangle inside `0..w, 0..h`, if any.
    fn clip(self, w: i64, h: i64) -> Option<Rect> {
        let x = self.x.max(0);
        let y = self.y.max(0);
        let r = self.right().min(w);
        let b = self.bottom().min(h);
        (r > x && b > y).then_some(Rect {
            x,
            y,
            w: r - x,
            h: b - y,
        })
    }
}

/// How a layer's placement changes over time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    /// Stays at the layer's own placement.
    Still,
    /// Eased from one placement to another, then stays at `to`.
    Glide {
        from: Placement,
        to: Placement,
        start_ns: u64,
        duration_ns: u64,
    },
    /// Diagonal, kept inside the scene. Ignores the layer's own placement.
    Bounce { dots_per_second: f64 },
}

/// A canvas with a position and a way of moving.
#[derive(Clone, Debug)]
pub struct Layer {
    pub canvas: Canvas,
    pub placement: Placement,
    pub motion: Motion,
    /// A colour treated as see-through.
    pub clear: Option<Rgb>,
    pub visible: bool,
}

/// Layers over a background, back to front.
#[derive(Clone, Debug)]
pub struct Scene {
    pub w: usize,
    pub h: usize,
    pub background: Rgb,
    pub layers: Vec<Layer>,
}

/// Whole dots travelled by a bouncing layer at `t_ns`: `floor(t × dots_per_second)`.
fn bounce_dots(t_ns: u64, dots_per_second: f64) -> u64 {
    let d = (t_ns as f64 * dots_per_second / 1e9).floor();
    // `max` also turns NaN into 0.
    d.max(0.0) as u64
}

/// Triangle wave over `0..=room`: up, then back down.
fn triangle(d: u64, room: i64) -> i64 {
    if room <= 0 {
        return 0;
    }
    let room = room as u64;
    let m = d % (2 * room);
    (if m > room { 2 * room - m } else { m }) as i64
}

fn smoothstep(p: f64) -> f64 {
    p * p * (3.0 - 2.0 * p)
}

fn lerp_round(a: i64, b: i64, s: f64) -> i64 {
    (a as f64 + (b - a) as f64 * s).round() as i64
}

impl Scene {
    fn layer_place(&self, layer: &Layer, t_ns: u64) -> Placement {
        match layer.motion {
            Motion::Still => layer.placement,
            Motion::Glide {
                from,
                to,
                start_ns,
                duration_ns,
            } => {
                if t_ns < start_ns {
                    return from;
                }
                if t_ns - start_ns >= duration_ns {
                    return to;
                }
                let s = smoothstep((t_ns - start_ns) as f64 / duration_ns as f64);
                Placement {
                    x: lerp_round(from.x, to.x, s),
                    y: lerp_round(from.y, to.y, s),
                }
            }
            Motion::Bounce { dots_per_second } => {
                let d = bounce_dots(t_ns, dots_per_second);
                Placement {
                    x: triangle(d, self.w as i64 - layer.canvas.w as i64),
                    y: triangle(d, self.h as i64 - layer.canvas.h as i64),
                }
            }
        }
    }

    /// Where a layer sits at `t_ns`.
    pub fn place(&self, layer: usize, t_ns: u64) -> Placement {
        self.layer_place(&self.layers[layer], t_ns)
    }

    /// Draw the scene at `t_ns` onto `out`: the background, then each visible layer, back to front.
    pub fn compose(&self, t_ns: u64, out: &mut Canvas) {
        out.dots.fill(self.background);
        let (ow, oh) = (out.w as i64, out.h as i64);
        for layer in self.layers.iter().filter(|l| l.visible) {
            let p = self.layer_place(layer, t_ns);
            let (lw, lh) = (layer.canvas.w as i64, layer.canvas.h as i64);
            let (x0, x1) = (p.x.max(0), (p.x + lw).min(ow));
            let (y0, y1) = (p.y.max(0), (p.y + lh).min(oh));
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let n = (x1 - x0) as usize;
            for y in y0..y1 {
                let s = ((y - p.y) * lw + (x0 - p.x)) as usize;
                let d = (y * ow + x0) as usize;
                let src = &layer.canvas.dots[s..s + n];
                let dst = &mut out.dots[d..d + n];
                match layer.clear {
                    None => dst.copy_from_slice(src),
                    Some(c) => {
                        for (o, &i) in dst.iter_mut().zip(src) {
                            if i != c {
                                *o = i;
                            }
                        }
                    }
                }
            }
        }
    }

    /// The area covered before or after by layers whose place changed between the two
    /// instants, clipped to the scene. `None` when nothing moved.
    pub fn moved(&self, t0_ns: u64, t1_ns: u64) -> Option<Rect> {
        let mut area: Option<Rect> = None;
        for layer in self.layers.iter().filter(|l| l.visible) {
            let (a, b) = (
                self.layer_place(layer, t0_ns),
                self.layer_place(layer, t1_ns),
            );
            if a == b {
                continue;
            }
            let (w, h) = (layer.canvas.w as i64, layer.canvas.h as i64);
            for p in [a, b] {
                let r = Rect {
                    x: p.x,
                    y: p.y,
                    w,
                    h,
                };
                if let Some(r) = r.clip(self.w as i64, self.h as i64) {
                    area = Some(area.map_or(r, |u| u.union(r)));
                }
            }
        }
        area
    }

    /// When any placement next changes after `t_ns`; `None` if nothing will move.
    pub fn next_move_ns(&self, t_ns: u64) -> Option<u64> {
        self.layers
            .iter()
            .filter(|l| l.visible)
            .filter_map(|l| self.next_layer_move(l, t_ns))
            .min()
    }

    fn next_layer_move(&self, layer: &Layer, t_ns: u64) -> Option<u64> {
        match layer.motion {
            Motion::Still => None,
            Motion::Glide {
                start_ns,
                duration_ns,
                ..
            } => {
                let end = start_ns.saturating_add(duration_ns);
                let here = self.layer_place(layer, t_ns);
                if t_ns >= end || self.layer_place(layer, end) == here {
                    return None;
                }
                // Each axis only ever moves one way, so "differs from here" is monotone
                // in time: find the first instant it holds.
                let (mut lo, mut hi) = (t_ns + 1, end);
                while lo < hi {
                    let mid = lo + (hi - lo) / 2;
                    if self.layer_place(layer, mid) != here {
                        hi = mid;
                    } else {
                        lo = mid + 1;
                    }
                }
                Some(lo)
            }
            Motion::Bounce { dots_per_second } => {
                let still = |room: i64| room <= 0;
                if still(self.w as i64 - layer.canvas.w as i64)
                    && still(self.h as i64 - layer.canvas.h as i64)
                {
                    return None;
                }
                if dots_per_second.is_nan() || dots_per_second <= 0.0 {
                    return None;
                }
                let next = bounce_dots(t_ns, dots_per_second) + 1;
                let mut t = (next as f64 * 1e9 / dots_per_second).ceil() as u64;
                // Correct any rounding in the float arithmetic.
                while bounce_dots(t, dots_per_second) < next {
                    t += 1;
                }
                while t > 0 && bounce_dots(t - 1, dots_per_second) >= next {
                    t -= 1;
                }
                Some(t.max(t_ns + 1))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BG: Rgb = 0x101010;
    const RED: Rgb = 0xFF0000;
    const BLUE: Rgb = 0x0000FF;
    const KEY: Rgb = 0x00FF00;

    fn layer(w: usize, h: usize, fill: Rgb, at: (i64, i64), motion: Motion) -> Layer {
        Layer {
            canvas: Canvas::new(w, h, fill),
            placement: Placement { x: at.0, y: at.1 },
            motion,
            clear: None,
            visible: true,
        }
    }

    fn scene(w: usize, h: usize, layers: Vec<Layer>) -> Scene {
        Scene {
            w,
            h,
            background: BG,
            layers,
        }
    }

    fn composed(s: &Scene, t: u64) -> Canvas {
        let mut out = Canvas::new(s.w, s.h, 0);
        s.compose(t, &mut out);
        out
    }

    #[test]
    fn overlapping_layers_without_clear() {
        let s = scene(
            10,
            10,
            vec![
                layer(4, 4, RED, (1, 1), Motion::Still),
                layer(4, 4, BLUE, (3, 3), Motion::Still),
            ],
        );
        let c = composed(&s, 0);
        assert_eq!(c.get(0, 0), Some(BG));
        assert_eq!(c.get(1, 1), Some(RED));
        assert_eq!(c.get(3, 3), Some(BLUE)); // front layer wins in the overlap
        assert_eq!(c.get(6, 6), Some(BLUE));
        assert_eq!(c.get(7, 7), Some(BG));
        assert_eq!(c.get(5, 1), Some(BG));
    }

    #[test]
    fn overlapping_layers_with_clear() {
        let mut front = layer(4, 4, KEY, (3, 3), Motion::Still);
        front.canvas.fill_rect(2, 2, 2, 2, BLUE);
        front.clear = Some(KEY);
        let s = scene(10, 10, vec![layer(4, 4, RED, (1, 1), Motion::Still), front]);
        let c = composed(&s, 0);
        assert_eq!(c.get(3, 3), Some(RED)); // see-through: the layer behind shows
        assert_eq!(c.get(4, 4), Some(RED));
        assert_eq!(c.get(5, 5), Some(BLUE)); // opaque part of the front layer
        assert_eq!(c.get(6, 6), Some(BLUE));
        assert_eq!(c.get(6, 3), Some(BG)); // see-through over bare background
    }

    #[test]
    fn hidden_layers_and_edges_are_clipped() {
        let mut hidden = layer(4, 4, BLUE, (0, 0), Motion::Still);
        hidden.visible = false;
        let s = scene(5, 5, vec![hidden, layer(4, 4, RED, (-2, 3), Motion::Still)]);
        let c = composed(&s, 0);
        assert_eq!(c.get(0, 0), Some(BG));
        assert_eq!(c.get(0, 3), Some(RED));
        assert_eq!(c.get(1, 4), Some(RED));
        assert_eq!(c.get(2, 3), Some(BG));
        // Entirely off the scene draws nothing and does not panic.
        let s = scene(5, 5, vec![layer(4, 4, RED, (9, 9), Motion::Still)]);
        assert_eq!(composed(&s, 0).dots, vec![BG; 25]);
    }

    fn glide() -> Scene {
        scene(
            200,
            100,
            vec![layer(
                10,
                10,
                RED,
                (0, 0),
                Motion::Glide {
                    from: Placement { x: 10, y: 20 },
                    to: Placement { x: 110, y: 70 },
                    start_ns: 1_000,
                    duration_ns: 1_000_000,
                },
            )],
        )
    }

    #[test]
    fn glide_ends_are_exact() {
        let s = glide();
        let from = Placement { x: 10, y: 20 };
        let to = Placement { x: 110, y: 70 };
        assert_eq!(s.place(0, 0), from);
        assert_eq!(s.place(0, 1_000), from);
        assert_eq!(s.place(0, 1_000_999), s.place(0, 1_000_999));
        assert_eq!(s.place(0, 1_001_000), to);
        assert_eq!(s.place(0, 9_999_999_999), to);
        // Eased: slower than linear at the start, never leaves the line between the ends.
        let early = s.place(0, 1_000 + 100_000);
        assert!(early.x < 10 + 10 && early.x >= 10, "{early:?}");
        let mid = s.place(0, 1_000 + 500_000);
        assert_eq!(mid, Placement { x: 60, y: 45 });
        let mut last = from;
        for i in 0..=1000 {
            let p = s.place(0, 1_000 + i * 1_000);
            assert!(p.x >= last.x && p.y >= last.y);
            last = p;
        }
    }

    #[test]
    fn glide_with_no_duration_jumps() {
        let mut s = glide();
        s.layers[0].motion = Motion::Glide {
            from: Placement { x: 1, y: 1 },
            to: Placement { x: 5, y: 5 },
            start_ns: 100,
            duration_ns: 0,
        };
        assert_eq!(s.place(0, 99), Placement { x: 1, y: 1 });
        assert_eq!(s.place(0, 100), Placement { x: 5, y: 5 });
        assert_eq!(s.next_move_ns(0), Some(100));
        assert_eq!(s.next_move_ns(100), None);
    }

    #[test]
    fn glide_next_move_is_the_first_change() {
        let s = glide();
        let mut t = 0;
        let mut steps = 0;
        while let Some(n) = s.next_move_ns(t) {
            assert!(n > t);
            assert_eq!(s.place(0, n - 1), s.place(0, t), "moved early");
            assert_ne!(s.place(0, n), s.place(0, t), "did not move");
            t = n;
            steps += 1;
        }
        assert!(steps >= 100); // at least the 100 whole dots across
        assert_eq!(s.place(0, t), Placement { x: 110, y: 70 });
        assert_eq!(s.next_move_ns(t), None);
    }

    fn bounce(dps: f64) -> Scene {
        scene(
            320,
            180,
            vec![layer(
                288,
                162,
                RED,
                (0, 0),
                Motion::Bounce {
                    dots_per_second: dps,
                },
            )],
        )
    }

    #[test]
    fn bounce_stays_inside_and_steps_together() {
        for dps in [10.0, 37.5, 60.0] {
            let s = bounce(dps);
            let sample = 1_000_000_000 / 60;
            let mut last = s.place(0, 0);
            assert_eq!(last, Placement { x: 0, y: 0 });
            for i in 1..=(60 * 600u64) {
                let p = s.place(0, i * sample);
                assert!(p.x >= 0 && p.y >= 0, "{p:?}");
                assert!(p.x + 288 <= 320 && p.y + 162 <= 180, "{p:?}");
                assert_eq!(p.x != last.x, p.y != last.y, "axes stepped apart at {p:?}");
                last = p;
            }
        }
    }

    #[test]
    fn bounce_reaches_the_edges_and_turns() {
        let s = bounce(1.0);
        let at = |sec: u64| s.place(0, sec * 1_000_000_000);
        assert_eq!(at(18), Placement { x: 18, y: 18 });
        assert_eq!(at(32), Placement { x: 32, y: 4 });
        assert_eq!(at(33), Placement { x: 31, y: 3 });
        assert_eq!(at(36), Placement { x: 28, y: 0 }); // y has come all the way back up
    }

    #[test]
    fn bounce_ignores_a_layer_too_big_for_the_scene() {
        let s = scene(
            10,
            10,
            vec![layer(
                20,
                5,
                RED,
                (0, 0),
                Motion::Bounce {
                    dots_per_second: 5.0,
                },
            )],
        );
        for i in 0..100 {
            let p = s.place(0, i * 123_456_789);
            assert_eq!(p.x, 0);
            assert!((0..=5).contains(&p.y));
        }
    }

    #[test]
    fn next_move_for_still_layers_is_none() {
        let s = scene(
            10,
            10,
            vec![
                layer(2, 2, RED, (0, 0), Motion::Still),
                layer(2, 2, BLUE, (4, 4), Motion::Still),
            ],
        );
        assert_eq!(s.next_move_ns(0), None);
        assert_eq!(s.next_move_ns(12345), None);
        let empty = scene(10, 10, vec![]);
        assert_eq!(empty.next_move_ns(0), None);
    }

    #[test]
    fn next_move_for_bounce_at_ten_dots_a_second() {
        let s = bounce(10.0);
        assert_eq!(s.next_move_ns(0), Some(100_000_000));
        // From a step, the next is 100 ms later.
        assert_eq!(s.next_move_ns(100_000_000), Some(200_000_000));
        assert_eq!(s.next_move_ns(100_000_001), Some(200_000_000));
        assert_eq!(s.next_move_ns(199_999_999), Some(200_000_000));
        // And it really is the first instant the placement changes.
        for t in [0u64, 100_000_000, 333_333_333, 7_654_321_987] {
            let n = s.next_move_ns(t).unwrap();
            assert_ne!(s.place(0, n), s.place(0, t));
            assert_eq!(s.place(0, n - 1), s.place(0, t));
        }
    }

    #[test]
    fn next_move_for_bounce_with_nowhere_to_go_is_none() {
        let s = scene(
            10,
            10,
            vec![layer(
                10,
                10,
                RED,
                (0, 0),
                Motion::Bounce {
                    dots_per_second: 5.0,
                },
            )],
        );
        assert_eq!(s.next_move_ns(0), None);
        assert_eq!(s.moved(0, 10_000_000_000), None);
    }

    #[test]
    fn next_move_takes_the_soonest_and_ignores_hidden() {
        let mut s = bounce(10.0);
        s.layers.push(layer(
            4,
            4,
            BLUE,
            (0, 0),
            Motion::Glide {
                from: Placement { x: 0, y: 0 },
                to: Placement { x: 50, y: 0 },
                start_ns: 0,
                duration_ns: 1_000_000_000,
            },
        ));
        let n = s.next_move_ns(0).unwrap();
        assert!(n < 100_000_000, "the glide moves sooner: {n}");
        s.layers[1].visible = false;
        assert_eq!(s.next_move_ns(0), Some(100_000_000));
    }

    #[test]
    fn moved_is_none_when_nothing_moved() {
        let s = scene(10, 10, vec![layer(2, 2, RED, (3, 3), Motion::Still)]);
        assert_eq!(s.moved(0, 1_000_000_000), None);
        // A glide that has not started, and one that has finished.
        let g = glide();
        assert_eq!(g.moved(0, 500), None);
        assert_eq!(g.moved(2_000_000, 3_000_000), None);
        // Same instant.
        assert_eq!(bounce(10.0).moved(5, 5), None);
        // A bounce step too small to be a whole dot.
        assert_eq!(bounce(10.0).moved(0, 99_999_999), None);
    }

    #[test]
    fn moved_covers_before_and_after() {
        let s = bounce(10.0);
        let r = s.moved(0, 100_000_000).unwrap();
        assert_eq!(
            r,
            Rect {
                x: 0,
                y: 0,
                w: 289,
                h: 163
            }
        );
        let g = glide();
        let r = g.moved(0, 2_000_000).unwrap();
        assert_eq!(
            r,
            Rect {
                x: 10,
                y: 20,
                w: 110,
                h: 60
            }
        );
        // Only layers that moved count.
        let mut s = bounce(10.0);
        s.layers.push(layer(5, 5, BLUE, (300, 100), Motion::Still));
        let r = s.moved(0, 100_000_000).unwrap();
        assert_eq!((r.w, r.h), (289, 163));
    }

    #[test]
    fn moved_is_clipped_to_the_scene() {
        let s = scene(
            20,
            20,
            vec![layer(
                10,
                10,
                RED,
                (0, 0),
                Motion::Glide {
                    from: Placement { x: -5, y: 0 },
                    to: Placement { x: 30, y: 0 },
                    start_ns: 0,
                    duration_ns: 100,
                },
            )],
        );
        let r = s.moved(0, 100).unwrap();
        assert_eq!(
            r,
            Rect {
                x: 0,
                y: 0,
                w: 5,
                h: 10
            }
        );
    }

    #[test]
    fn composing_moves_without_redrawing() {
        let s = bounce(10.0);
        let a = composed(&s, 0);
        let b = composed(&s, 100_000_000);
        assert_eq!(a.get(0, 0), Some(RED));
        assert_eq!(a.get(319, 179), Some(BG));
        assert_eq!(b.get(0, 0), Some(BG));
        assert_eq!(b.get(1, 1), Some(RED));
        assert_eq!(b.get(288, 162), Some(RED));
        assert_eq!(a.get(288, 162), Some(BG));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn composing_a_288_by_162_layer_takes_under_a_millisecond() {
        let mut s = bounce(30.0);
        s.layers[0].clear = Some(KEY);
        let mut out = Canvas::new(320, 180, 0);
        let n = 200u32;
        let start = std::time::Instant::now();
        for i in 0..n {
            s.compose(u64::from(i) * 16_666_667, &mut out);
            std::hint::black_box(&out);
        }
        let each = start.elapsed() / n;
        assert!(each.as_micros() < 1000, "took {each:?}");
        s.layers[0].clear = None;
        let start = std::time::Instant::now();
        for i in 0..n {
            s.compose(u64::from(i) * 16_666_667, &mut out);
            std::hint::black_box(&out);
        }
        let each = start.elapsed() / n;
        assert!(each.as_micros() < 1000, "took {each:?}");
    }
}
