# Cadence of the benchmark in Apple's Terminal 2.15, filmed

macOS 27.2, Apple M1 Max, built-in Liquid Retina XDR display (3456 × 2234), dark mode,
Menlo, 160 × 45 cells. Filmed 2026-11-11 with `witness-film` through
`rigs/apple-terminal/with-screen.sh`, release builds of `main` at 306ef6c (after #48) with this
change's witness:

```
witness-film --font <12|6> --cols 160 --rows 45 --seconds 31 --fps 120 -- /bin/sh -c \
  "phosphor play bounce --profile apple --rung <half|quarter> --strip \
   --full-colour-layer card.png --seconds 30 --report run.json [--no-govern]; sleep 10"
```

`card.png` is the kernel's `test_card(320, 180, 0)` written with `witness::still::save_photo`,
as in #46. The program plays 30 seconds at its default beat of 60 a second and default speed,
writes its report, and the window then holds still until the film has ended. (The film tool
closes the window when it is done, and the program restores the terminal and leaves on that
signal without writing its report. So the program has to finish first.) The film is read from
its first frame with a readable frame counter strip to its last, which is the program's 30 s.

**Accepted** below is what the program's report says the terminal took. **Seen** is what the
film shows. Nothing is called smooth.

## Results

| Rung | Font | Governor | Plan chosen | Sent (accepted a second) | Shown (seen a second) | Repeats | Skips | Torn | Longest hold | Median hold | Film rate |
|---|---|---|---|---|---|---|---|---|---|---|---|
| half | 12 | on | half, 55 runs a row, full colour | 914 (30.5) | 425 (14.2) | 3170 | 488 | 0 | 141.7 ms (17 film frames) | 66.7 ms | 120 asked, 72.7 captured |
| half | 12 | off | — | 926 (30.9) | 412 (13.7) | 3186 | 513 | 0 | 141.7 ms (17) | 66.7 ms | 120 asked, 71.4 captured |
| quarter | 12 | on | quarter, 55 runs a row, full colour | 1249 (41.6) | 503 (16.8) | 3095 | 746 | 0 | 150.0 ms (18) | 58.3 ms | 120 asked, 72.2 captured |
| quarter | 12 | off | — | 1263 (42.1) | 485 (16.2) | 3112 | 776 | 0 | 150.0 ms (18) | 58.3 ms | 120 asked, 72.8 captured |
| half | 6 | on | half, 55 runs a row, full colour | 919 (30.6) | 428 (14.3) | 3170 | 490 | 0 | 141.7 ms (17) | 66.7 ms | 120 asked, 70.7 captured |
| half | 6 | off | — | 918 (30.6) | 411 (13.7) | 3186 | 505 | 0 | 150.0 ms (18) | 66.7 ms | 120 asked, 70.1 captured |
| quarter | 6 | on | quarter, 55 runs a row, full colour | 1245 (41.5) | 506 (16.9) | 3094 | 739 | 0 | 133.3 ms (16) | 58.3 ms | 120 asked, 66.4 captured |
| quarter | 6 | off | — | 1254 (41.8) | 484 (16.1) | 3115 | 770 | 0 | 141.7 ms (17) | 58.3 ms | 120 asked, 64.9 captured |

In every run, shown plus skips equals the frames the program sent (or is one or two short: frames
sent before the film's first readable frame), so the film read the strip correctly.

From the program's reports:

| Rung | Font | Governor | Frames held (sent in part) | Beats unchanged | Beats late | Beats waiting for an answer | Answers over budget | Answer median / p99 / worst |
|---|---|---|---|---|---|---|---|---|
| half | 12 | on | 300 | 609 | 72 | 205 | 154 | 0.19 / 32.6 / 34.4 ms |
| half | 12 | off | — | 617 | 71 | 186 | 167 | 0.21 / 35.5 / 38.3 ms |
| quarter | 12 | on | 301 | 309 | 54 | 188 | 141 | 0.17 / 27.7 / 29.9 ms |
| quarter | 12 | off | — | 314 | 43 | 180 | 170 | 0.16 / 29.3 / 33.0 ms |
| half | 6 | on | 300 | 632 | 76 | 173 | 129 | 0.18 / 31.4 / 35.8 ms |
| half | 6 | off | — | 628 | 59 | 195 | 167 | 0.21 / 35.7 / 37.2 ms |
| quarter | 6 | on | 298 | 296 | 70 | 189 | 128 | 0.16 / 26.7 / 29.1 ms |
| quarter | 6 | off | — | 303 | 57 | 186 | 188 | 0.16 / 29.9 / 34.1 ms |

## Was it seen at 60 frames a second?

No. Not one run.

| Governed run | Seen | At what size and richness |
|---|---|---|
| half, font 12 | 14.2 frames a second, 488 skipped in 30 s | 160 × 45 cells, 160 × 90 dots, full colour, up to 55 runs a row |
| quarter, font 12 | 16.8 frames a second, 746 skipped | 160 × 45 cells, 320 × 90 dots, full colour, up to 55 runs a row |
| half, font 6 | 14.3 frames a second, 490 skipped | as half, font 12 |
| quarter, font 6 | 16.9 frames a second, 739 skipped | as quarter, font 12 |

No governed run meets "0 skips over 30 seconds and a longest hold of at most two film frames".
Their longest holds are 16 to 18 film frames (133 to 150 ms). The governor makes almost no
difference to what is seen: half a frame a second more than without it, or less.

## Why it cannot: Apple's Terminal repaints about 22 times a second at most

The program's answer times say Terminal *read* each frame quickly (median 0.2 ms). An answer
does not mean the frame was painted. The film shows that Terminal paints far less often than it
reads, whatever is sent:

1. **The ticker, which sends almost nothing, is seen at 22 a second, at any grid size.**
   `witness-ticker` sends the strip and a 4-cell bar 60 times a second. Filmed for 10 s at font
   12:

   | Grid | Seen | Skips | Longest hold |
   |---|---|---|---|
   | 160 × 45 | 21.9 a second | 374 | 75.0 ms |
   | 80 × 24 | 21.7 a second | 379 | 75.0 ms |
   | 40 × 12 | 22.0 a second | 374 | 108.3 ms |
   | 160 × 45, film asked for 60 a second (58.9 captured) | 21.9 a second | 374 | 66.7 ms |

   A grid one fifteenth the size is painted no more often, so cost per cell or per run is not
   the limit. Filming at half the rate gives the same number, so filming is not the limit
   either. The screen recording delivers about 70 frames a second, more than three times what
   Terminal paints.

2. **The benchmark, slowed down, is seen in full at 15 a second and not above.** Governed,
   quarter rung, font 12, 15 s each. The 15-a-second run was filmed on 306ef6c; the 30 and 20
   ones on 88d0a67, before #48, which a refilm of the eight runs showed changes nothing here:

   | Beat asked | Plan chosen | Sent | Seen | Skips | Longest hold | Median hold |
   |---|---|---|---|---|---|---|
   | 30 a second | quarter, 97 runs a row, full colour | 393 (26.1 a second) | 15.0 a second | 167 | 141.7 ms | 66.7 ms |
   | 20 a second | quarter, 131 runs a row | 282 (18.8) | 14.0 | 72 | 133.3 ms | 66.7 ms |
   | 15 a second | quarter, 160 runs a row | 217 (14.5) | 14.5 | **0** | 150.0 ms | 66.7 ms |

   At 15 a second every frame sent is seen, but not on an even beat: holds still run from 33 to
   150 ms, because Terminal paints on its own clock, not the program's.

So on this Mac, Apple's Terminal shows at most about 22 frames a second of anything, and about
15 a second of the benchmark with a picture behind it. Above that, frames are read and then
painted over before they reach the screen. The governor keeps frames inside a budget measured by
answer times, and answers come back when Terminal has *read* a frame, not when it has painted
one. Nothing in the governor or the program can make Terminal paint more often, so no change was
made to either. A beat the governor could hold on film here would be below 15 a second, and even
that is uneven.

**The camera has since been checked** (`cadence-ghostty.md`). Pointed at Ghostty on the same Mac,
at the same grid size, the same camera sees the ticker at 50 to 57 frames a second, and the
benchmark on the eighth rung at 53 to 55. So the camera is not what holds Apple's Terminal at
about 22 a second: that ceiling is Apple's Terminal's own, and it is now seen, not only argued.

This is the stuttery picture first reported: at a 60-a-second beat, roughly two frames in three never
appear, and the ones that do stay up for 33 to 150 ms.

## How to read the numbers

- The film is counted in film frames of 8.3 ms (120 a second asked). The screen recording really
  delivered 65 to 73 frames a second. `ffmpeg` copied frames to keep the film's rate even, and
  most **repeats** are those copies. A hold is known to within one recorded frame, about 14 ms.
- A 60-a-second picture would change every 16.7 ms. With a recorded frame every 14 ms on
  average, the film can tell 60 a second from 15, but it cannot promise to catch every single
  16.7 ms frame. That does not matter here: Terminal's ceiling is a quarter of 60.
- **Torn** is 0 in every run: no film frame caught the strip half painted.
- "Beats unchanged" are beats on which the picture had not moved, so nothing was sent. At the
  default speed of 10 dots a second this is a third of the beats on the half rung and a sixth on
  the quarter. On its own it would give holds of two beats. Skips are frames that *were* sent.

## Changed to make the film readable

`witness::cadence::locate_strip` could not find the frame counter strip in these films. In
dark mode the title bar's text is white on black, and it changes when the program starts, so the
title came first. Beside a picture, the picture's still edges outvoted the edge between cells 17
and 18, and the strip was read a cell off. It now looks in every band for the place whose numbers
step forward like a counter, with edges on its cell borders and none inside. The ticker's strip is
found where it was before (x 20, 78 down, 252 × 30 at font 12).
