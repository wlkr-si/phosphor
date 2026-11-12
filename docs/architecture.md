# How the parts join

```
 app ──▶ Composition ──canvas──▶ thrift ──canvas──▶ fit ──grid──▶ encode ──bytes──▶ Surface ──▶ real screen
                                    ▲                ▲               │ tally            │ profile      │
                                    └──── Governor ──┴───────────────┘◀─────────────────┘              │
                                                                                                       ▼
                                     Bench (pretend terminal)                                Witness (photo, film)
```

One frame:

1. **Composition** places its layers for this instant and produces a **canvas**.
2. The **governor** decides how thrifty this frame must be. **Thrift** simplifies the
   canvas's colours to suit.
3. **Fit** turns the canvas into a **grid** for the rung in use.
4. **Encode** compares it with the grid last sent and writes the bytes for the damage,
   using only what the surface's **profile** allows. It returns a **tally**.
5. The governor prices the tally. If it fits the **budget** the **surface** sends it on
   the **beat**. If not, the governor sends the moving part now and leaves still detail
   to **develop** on later frames.

## Seams

Every part depends on `kernel` and on nothing else, until the program is wired together.
These are the functions other parts will call. Their signatures are already in the code.

| Seam | Signature |
|---|---|
| fit | `fit::exact::fit(&Canvas, Ladder, Option<&Grid>, &mut Grid)` |
| picture meant | `fit::exact::unfit(&Grid, Ladder) -> Canvas` |
| thrift | `fit::thrift::thrift(&mut Canvas, Ladder, max_runs, ColourDepth) -> f64` |
| encode | `encode::encode(Option<&Grid>, &Grid, &Profile, &mut Vec<u8>) -> Tally` |
| price | `PriceList::price(&Tally) -> f64` (kernel) |
| pretend terminal | `bench::StrictTerminal::{new, feed, grid}` |
| line to a terminal | `kernel::Wire` |

## Who may touch what

| Part | Reads files | Touches a terminal | Needs a Mac |
|---|---|---|---|
| kernel, compose, fit, encode, govern, bench | no | no | no |
| foundry | font and image files | no | no |
| surface | no | yes | no |
| witness | photographs | opens Apple's Terminal | yes |

## The reference surface

Apple's Terminal. See `docs/measurements/apple-terminal-2.15.md`. In short: it draws half
and quarter blocks only; it has no atomic frames; its repeat command is wrong for block
characters; and it charges by the run, not by the byte.

## The frame counter strip

To film cadence, the program under test marks every frame so the film can tell them apart.
The strip is the first 16 cells of the top row. Each cell is a space whose paper is white
(a 1) or black (a 0). Together they spell the frame number, modulo 65,536, most
significant bit first. Cell 17 is always white and cell 18 always black, so a reader can
find the strip's scale and tell a torn frame from a whole one.

## Registration marks

To read dots back from a photograph, the program under test paints the four corner cells
of the grid as spaces with paper 0xFF00FF. The witness finds those four marks, takes their
outer edges as the grid's rectangle, and divides by columns and rows. Cell sizes are often
fractions of a pixel, so never assume whole numbers.
