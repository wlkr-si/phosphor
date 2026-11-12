# Apple's Terminal 2.15, measured

macOS 27.2, built-in 3456 × 2234 display (1728 × 1117 points), Menlo. Measured 2026-11-11
with `rigs/apple-terminal/`. Rates are **accepted** rates: the time until the terminal
answered. Nothing here has been filmed.

## What it can draw

| Question | Answer |
|---|---|
| Full colour | Yes |
| Half and quarter blocks | Yes, one cell wide, tiling cleanly |
| Sixth and eighth blocks | No. Boxed question marks. |
| Atomic frames | No |
| Repeat command on block characters | Wrong. Corrupts the picture. |
| Erase, relative cursor moves, bridging gaps | Correct |
| Smallest cell | 1 × 2 points at font size 1: 1708 × 516 cells, 1708 × 1032 dots on the half rung |

## What it charges

160 × 45 cells, half rung, every cell sent on every frame.

| What was sent | Bytes a frame | Frames a second | Time a frame |
|---|---|---|---|
| Full colour, both colours on every cell | 250 kB | 8.3 | 120 ms |
| The same, every colour instruction sent twice | 478 kB | 6.6 | 152 ms |
| 256 colours, both on every cell | 158 kB | 14.4 | 69 ms |
| 8 colours, both on every cell | 80 kB | 13.5 | 74 ms |
| Full colour, colour changing every 8 cells | 38 kB | 237 | 4.2 ms |
| Full colour, colour changing every 40 cells | 27 kB | 589 | 1.7 ms |
| Fixed colours, only the shapes changing | 22 kB | 979 | 1.0 ms |

Reading:

- **Runs cost, bytes barely do.** 7,200 cells with no colour changes take 1 ms. The same
  cells with 7,200 colour changes take 120 ms.
- **Doubling the bytes adds a quarter.** 250 kB to 478 kB costs 32 ms more.
- **Fewer bytes per colour does not help by itself.** 8 colours use half the bytes of 256
  and are no faster. Indexed colour is about 1.7 times faster than full colour.
- **Runs on a crowded row cost more each.** 4 runs a row: about 4 µs each. 160 a row:
  about 17 µs each.

First fit, used by `Profile::apple_terminal_2_15()`: 46 ns a byte, 1,700 ns a run, 93 ns per
unit of crowding.

## Whole pictures

| What | Accepted |
|---|---|
| One ink, whole picture moving, 320 × 90 dots | 60 a second |
| One ink, whole picture moving, 800 × 258 dots | 59.5 a second, stalls up to 0.1 s |
| One ink, whole picture moving, 1138 × 400 dots | 9 a second |
| A 16 × 8-cell thing over a still 569 × 200-cell picture | 71 steps a second, stalls up to 0.28 s |
| Terminal-side scrolling of a 569 × 200-cell picture | 4 steps a second |
| One still full-colour picture, 1708 × 1032 dots | 61 s to paint |

## The price sweep, font 12

`rigs/apple-terminal/fb.rs --sweep`, 160 × 45 cells, half rung, a frame sent whole and answered
before the next. Median time a frame over up to 120 frames or 3 seconds. Every cell of every
changing row gets new colours each frame, so no cell is spared by being unchanged; neighbouring
runs always differ in ink and paper. Measured 2026-11-11 on a machine also busy with other
work, so read each figure as plus or minus 10 percent. All three fonts, with bytes, runs,
crowding and the 99th percentile, are in `apple-terminal-2.15-prices.csv`.

| Runs a row | Runs a frame | Bytes a frame (full) | Full colour | 256 colours | Full, every fourth row | 256, every fourth row |
|---|---|---|---|---|---|---|
| 1 | 45 | 23 kB | 1.0 ms | 0.9 ms | 0.3 ms | 0.3 ms |
| 2 | 90 | 25 kB | 1.1 ms | 1.1 ms | 0.3 ms | 0.3 ms |
| 4 | 180 | 28 kB | 1.4 ms | 1.4 ms | 0.4 ms | 0.4 ms |
| 8 | 360 | 34 kB | 2.1 ms | 2.1 ms | 0.6 ms | 0.6 ms |
| 16 | 720 | 45 kB | 3.4 ms | 3.5 ms | 0.9 ms | 0.9 ms |
| 32 | 1440 | 69 kB | 33.7 ms | 32.5 ms | 1.6 ms | 1.5 ms |
| 64 | 2880 | 116 kB | 11.8 ms | 11.1 ms | 3.2 ms | 2.9 ms |
| 1 per cell (160) | 7200 | 256 kB | 171.8 ms | 165.6 ms | 8.3 ms | 7.6 ms |

- One run per cell, every row changing, is 172 ms in full colour, above the 120 ms
  the first rig run gave. That run's picture slid one dot a frame and left many cells identical
  to the last frame. Here nothing is spared.
- Eight runs a row (colour changing every 20 cells) is 2.1 ms.
- Time is not smooth in the run count. At 32 runs a row the answer came at 33 ms, about two 60 Hz display
  refreshes, in both depths and on repeat runs; at 64 runs it came at 12 ms. The fit should not
  trust any single point.
- Larger grids cost more per run as well: one run per cell takes 1.9 s a frame at font 8
  (341 × 103) and 4.9 s at font 6 (427 × 129).
