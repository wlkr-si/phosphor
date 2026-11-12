# The benchmark paced by paint rate in Apple's Terminal 2.15, filmed

macOS 27.2, Apple M1 Max, built-in Liquid Retina XDR display, dark mode, Menlo 12, 160 × 45
cells, quarter rung, governed. Filmed 2026-11-11 with `witness-film` from `main` at 37d2abd,
through `rigs/apple-terminal/with-screen.sh`, release builds of this change:

```
witness-film --font 12 --cols 160 --rows 45 --seconds 31 --fps 120 -- /bin/sh -c \
  "phosphor play bounce --profile apple --rung quarter --strip --full-colour-layer card.png \
   --seconds 30 --report run.json [--fps <beat>]; sleep 10"
```

`card.png` is the kernel's `test_card(320, 180, 0)`, as in `cadence-bounce.md`. The program
plays 30 s and writes its report; the window then holds still until the film ends.

**Sent** is from the program's report: what the terminal accepted. **Shown** and **skips** are
read from the film: what was seen. Holds are how long each frame shown stayed on screen. The
first and last holds are left out: they are the wait before the program starts and the still
window after it ends. The film is counted in film frames of 8.3 ms, but the screen recording
really delivered 66 to 79 frames a second, so a hold is known to within about 14 ms.

## Step 4: which beats are seen in full

| Beat asked | Plan: runs a row | Sent (accepted a second) | Shown (seen a second) | Skips | Torn | Shortest hold | Median hold | Longest hold | Middle 80% of holds |
|---|---|---|---|---|---|---|---|---|---|
| 24 | 114 | 669 (22.3) | 436 (14.5) | 231 | 0 | 16.7 ms | 66.7 ms | 158.3 ms | 50.0–83.3 ms |
| 22 | 122 | 619 (20.6) | 432 (14.4) | 187 | 0 | 33.3 ms | 66.7 ms | 175.0 ms | 50.0–83.3 ms |
| 20 | 131 | 571 (19.0) | 420 (14.0) | 150 | 0 | 33.3 ms | 66.7 ms | 150.0 ms | 50.0–91.7 ms |
| 18 | 141 | 522 (17.4) | 434 (14.5) | 88 | 0 | 33.3 ms | 66.7 ms | 166.7 ms | 50.0–91.7 ms |
| 15 | 160 | 440 (14.7) | 438 (14.6) | 2 | 0 | 41.7 ms | 66.7 ms | 158.3 ms | 58.3–83.3 ms |
| 15, again | 160 | 440 (14.7) | 440 (14.7) | **0** | 0 | 41.7 ms | 66.7 ms | 150.0 ms | 58.3–83.3 ms |
| 15, again | 160 | 440 (14.7) | 440 (14.7) | **0** | 0 | 41.7 ms | 66.7 ms | 158.3 ms | 50.0–83.3 ms |
| 15, again | 160 | 440 (14.7) | 440 (14.7) | **0** | 0 | 33.3 ms | 66.7 ms | 150.0 ms | 58.3–83.3 ms |
| 14 | 160 | 411 (13.7) | 411 (13.7) | **0** | 0 | 41.7 ms | 75.0 ms | 158.3 ms | 58.3–83.3 ms |
| 14, again | 160 | 411 (13.7) | 411 (13.7) | **0** | 0 | 41.7 ms | 66.7 ms | 158.3 ms | 58.3–83.3 ms |
| 12 | 160 | 354 (11.8) | 354 (11.8) | **0** | 0 | 58.3 ms | 83.3 ms | 175.0 ms | 75.0–91.7 ms |

At every beat from 18 to 24, the film shows the same thing: about 14.5 frames a second, whatever
was sent. Faster beats only add frames that are read and then painted over before anyone sees
them. At 15 a second, 3 takes of 4 showed every frame sent; the fourth missed 2 of 440. At 14
and 12, every frame was seen in every take.

The beat is a little above the frames sent (440 sent at a beat of 15 over 30 s, not 450): on 9
beats the picture had not moved, so nothing was sent, and one beat was late. Those 9 beats are
also why every run at 15 or slower has about 9 holds over 100 ms: a frame stays up for two beats
when the beat after it sends nothing. They are not missed paints.

As the beat slows, the governor's budget grows (1 / the beat), so it allows more runs a row: 114
at 24 a second, 160 (every column) at 15 and below. Richer frames cost Terminal more to paint,
which is part of why the benchmark is seen at about 14.5 a second while the plain ticker is seen
at about 22 (`cadence-ticker.md`).

## Step 5: trying to make the holds even

Terminal paints on its own clock, not the program's. Two ideas were filmed.

**A beat that is a whole fraction of 120** (the display's refresh rate): 24, 20, 15 and 12.
None is more even than its neighbours. 20 (a fraction) is no more even than 18 or 22 (not
fractions); 15 and 14 are about as even as each other.

**Each frame sent a fixed wait after the answer to the frame before**, instead of on the beat
(an option `--after-ack <ms>`, filmed and then removed; it is in the commits of pull request #52):

| Pacing | Sent (accepted a second) | Shown (seen a second) | Skips | Shortest hold | Median hold | Longest hold | Middle 80% of holds |
|---|---|---|---|---|---|---|---|
| 60 ms after each answer | 448 (14.9) | 448 (14.9) | 0 | 33.3 ms | 66.7 ms | 100.0 ms | 58.3–83.3 ms |
| 70 ms after each answer | 379 (12.6) | 379 (12.6) | 0 | 50.0 ms | 75.0 ms | 100.0 ms | 66.7–91.7 ms |
| On a beat of 15 (four takes) | 440 (14.7) | 438–440 (14.6–14.7) | 0–2 | 33.3–41.7 ms | 66.7 ms | 150.0–158.3 ms | 50.0–83.3 ms |

How much of each run's holds fall within 17 ms (two film frames) of its median: 95% and 93%
paced from answers; 92% to 96% on a beat of 15; 90% and 92% at 14. That is the same, within
what the film can tell.

Paced from answers, no hold is over 100 ms. That is not because the answers line up with
painting: an answer comes 0.25 ms after a frame goes out, when Terminal has read it, long
before it paints. It is because, off the beat, a frame that would show no change is tried
again at once instead of waiting a whole beat, so the two-beat holds above disappear. The
middle of the holds is no tighter. Because neither idea made the holds measurably more even,
the beat was kept and the option removed.

## What was set

`Profile::apple_terminal_2_15().paint_hz` is **14**. At 15 a second the benchmark was seen with
0 skips in four takes of six, and the other two missed 1 and 2 frames; at 14 it was seen with 0
skips in every take, five of five. So 14 is the fastest beat at which every frame sent was seen
over 30 seconds, every time it was filmed. With no `--fps`, the program now plays at that beat and
its report says `"fps":14,"fps_from":"paint rate","budget_ms":71.429`.

Filmed with no `--fps` at all, after the change:

| Run | Beat (from) | Sent (accepted a second) | Shown (seen a second) | Skips | Torn | Shortest hold | Median hold | Longest hold | Middle 80% of holds |
|---|---|---|---|---|---|---|---|---|---|
| 1 | 14 (paint rate) | 411 (13.7) | 411 (13.7) | **0** | 0 | 41.7 ms | 66.7 ms | 158.3 ms | 66.7–83.3 ms |
| 2 | 14 (paint rate) | 411 (13.7) | 411 (13.7) | **0** | 0 | 41.7 ms | 66.7 ms | 158.3 ms | 66.7–83.3 ms |
| 3 | 14 (paint rate) | 411 (13.7) | 411 (13.7) | **0** | 0 | 41.7 ms | 66.7 ms | 158.3 ms | 66.7–83.3 ms |

Two earlier films with the paint rate set to 15, also with no `--fps`, showed 1 skip and 0 skips
(counted among the six takes at 15 above).

How long each of the 1,227 frames shown in those three films stayed on screen (the beat is
71.4 ms):

| Hold | 41.7 | 50.0 | 58.3 | 66.7 | 75.0 | 83.3 | 91.7 | 100.0 | 116.7 to 158.3 ms |
|---|---|---|---|---|---|---|---|---|---|
| Frames | 7 | 29 | 54 | 571 | 306 | 186 | 46 | 1 | 27 |

## Plainly

The governed benchmark was **seen** in Apple's Terminal with no skipped frames at 14 frames a
second, in five 30-second films of five. That is the fastest beat at which that held every time;
faster beats are accepted but not seen, because Terminal paints this picture only about 14.5
times a second.

It is close to even, but not even. 91% of frames stayed up between 58 and 83 ms, against a beat
of 71 ms, which is within what the film can tell. The rest are uneven: a few as short as 42 ms,
and about 9 frames in every 30 s that stay up twice as long (117 to 158 ms), because on those
beats the picture had not moved and nothing was sent. Nothing here is called smooth.
