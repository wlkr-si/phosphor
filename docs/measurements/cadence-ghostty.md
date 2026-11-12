# Cadence in Ghostty 1.4.0, filmed: checking the camera

**The camera can see 57 frames a second.** The ticker, which paints 60 frames a second, was
seen at 55.0, 56.9 and 50.2 a second in Ghostty, filmed by the same camera that sees it at
about 22 a second in Apple's Terminal.

**The benchmark was not seen at 60 in Ghostty, in any of the four runs.** On the eighth rung
it was seen at 53.1 and 55.0 a second, as close to 60 as the ticker itself is seen, so the
film cannot tell those two runs apart from 60. On the quarter rung the program sent only 48
frames a second and 40 to 41 were seen.

So the camera is not what holds Apple's Terminal at about 22 frames a second: pointed at
Ghostty on the same Mac, at the same grid size, it sees more than twice that.

macOS 27.2, Apple M1 Max, built-in Liquid Retina XDR display (3456 × 2234), dark mode, 160 × 45
cells, font size 12. Filmed 2026-11-11 with `witness-film --terminal ghostty` through
`rigs/apple-terminal/with-screen.sh`, release builds of `main` at 37d2abd with this change's
witness. `rigs/ghostty/open.sh` opens each test window in a Ghostty of its own and does not
read the user's Ghostty settings, so the window has Ghostty's default font and colours:
cells 14 × 32 pixels (7 × 16 points), the strip 4 pixels from the window's left edge and
68 down.

**Accepted** below is what a program sent and the terminal took. **Seen** is what the film
shows. Nothing is called smooth.

## The ticker

```
witness-film --terminal ghostty --font 12 --cols 160 --rows 45 --seconds 10 -- \
  witness-ticker --seconds 30 --cols 160
```

`witness-ticker` paints the frame counter strip and a bar moving one cell a frame, 60 times a
second, with plain writes, as in `cadence-ticker.md`. It writes no report; "sent" is its 60 a
second over the film's length, and shown plus skips matches it in every run.

| Run | Sent (accepted a second) | Shown (seen a second) | Repeats | Skips | Torn | Longest hold | Median hold | Film rate |
|---|---|---|---|---|---|---|---|---|
| 1 | about 592 (60) | 543 (**55.0**) | 642 | 48 | 0 | 58.3 ms (7 film frames) | 16.7 ms | 120 asked, 75.0 captured |
| 2 | about 593 (60) | 562 (**56.9**) | 624 | 31 | 0 | 50.0 ms (6) | 16.7 ms | 120 asked, 79.8 captured |
| 3 | about 596 (60) | 498 (**50.2**) | 693 | 97 | 0 | 50.0 ms (6) | 16.7 ms | 120 asked, 78.3 captured |

How long each frame shown stayed on screen:

| Hold | Run 1 | Run 2 | Run 3 |
|---|---|---|---|
| 8.3 ms | 154 | 192 | 117 |
| 16.7 ms | 182 | 152 | 172 |
| 25.0 ms | 168 | 185 | 118 |
| 33.3 ms | 34 | 31 | 81 |
| 41.7 ms | 4 | 1 | 8 |
| 50.0 ms | 0 | 1 | 2 |
| 58.3 ms | 1 | 0 | 0 |

For comparison, the same ticker at the same grid in Apple's Terminal (`cadence-ticker.md`,
`cadence-bounce.md`): about 22 a second seen, about 375 skips in 10 s, median hold 41.7 ms.
Filmed again with this change's `witness-film --terminal apple` for 6 s: 21.9 a second seen,
225 skips, 0 torn.

## The benchmark

```
witness-film --terminal ghostty --font 12 --cols 160 --rows 45 --seconds 32 --fps 120 -- /bin/sh -c \
  "phosphor play bounce --rung <quarter|eighth> --strip [--full-colour-layer card.png] \
   --seconds 30 --report run.json; sleep 10"
```

`card.png` is the kernel's `test_card(320, 180, 0)` written with `witness::still::save_photo`,
as in `cadence-bounce.md`. The program asks the terminal its name, is told Ghostty, and uses
Ghostty's profile: all four rungs, atomic frames, and the cautious price list, since Ghostty's
prices have not been measured. It plays 30 seconds at its default beat of 60 a second and
default speed, governed, writes its report, and the window then holds still until the film
ends. The film is read from its first frame with a readable strip to its last: the program's
30 s.

| Rung | Picture behind | Plan chosen | Sent (accepted a second) | Shown (seen a second) | Repeats | Skips | Torn | Longest hold | Median hold | Film rate |
|---|---|---|---|---|---|---|---|---|---|---|
| quarter | none | quarter, 1 run a row, 8 colours | 1440 (48.0) | 1202 (**40.1**) | 2394 | 238 | 0 | 116.7 ms (14 film frames) | 25.0 ms | 120 asked, 70.3 captured |
| quarter | full colour | quarter, 1 run a row, 8 colours | 1441 (48.0) | 1228 (**41.0**) | 2370 | 213 | 0 | 116.7 ms (14) | 25.0 ms | 120 asked, 69.0 captured |
| eighth | none | eighth, 1 run a row, 8 colours | 1799 (60.0) | 1592 (**53.1**) | 2006 | 206 | 0 | 50.0 ms (6) | 16.7 ms | 120 asked, 70.1 captured |
| eighth | full colour | eighth, 1 run a row, 8 colours | 1793 (59.8) | 1651 (**55.0**) | 1949 | 142 | 0 | 41.7 ms (5) | 16.7 ms | 120 asked, 77.9 captured |

In every run shown plus skips equals the frames sent, so the film read the strip correctly.

From the program's reports:

| Rung | Picture behind | Frames held (sent in part) | Beats unchanged | Beats late | Answers over budget | Answer median / p99 / worst |
|---|---|---|---|---|---|---|
| quarter | none | 0 | 357 | 3 | 0 | 0.09 / 0.17 / 0.27 ms |
| quarter | full colour | 0 | 353 | 6 | 0 | 0.09 / 0.16 / 0.91 ms |
| eighth | none | 0 | 0 | 1 | 0 | 0.10 / 0.18 / 0.46 ms |
| eighth | full colour | 0 | 0 | 7 | 0 | 0.09 / 0.17 / 0.44 ms |

On the quarter rung a quarter of the beats had nothing new to send (the picture had not moved
by a whole dot), so the program sent 48 a second, not 60. On the eighth rung, with dots half as
tall, every beat moved the picture and 60 a second were sent.

The plan the governor chose deserves a note, though changing it is out of scope here: priced
with the cautious price list, it simplified every frame to one run a row in 8 colours, so the
picture behind barely changed what was sent. These films say how often Ghostty showed a frame,
not how rich a frame it can show at 60.

## Was the benchmark seen at 60 frames a second in Ghostty?

No. Not one run meets "0 skips over 30 seconds and a longest hold of at most two film frames".

| Run | Seen | What the film can and cannot say |
|---|---|---|
| quarter, no picture | 40.1 a second, 238 skipped | Below the 48 sent. Holds of 100 to 117 ms appear (12 frames in this run, 8 in the next), which the eighth-rung runs and the ticker never show, so this is not only the camera; the film cannot say whether Ghostty or the program's beat caused them. |
| quarter, full colour | 41.0 a second, 213 skipped | As above. |
| eighth, no picture | 53.1 a second, 206 skipped | Inside the range the ticker is seen at (50.2 to 56.9). The film cannot tell this from 60 painted. |
| eighth, full colour | 55.0 a second, 142 skipped | As above. |

## What the camera can and cannot show

- The screen recording delivered 69 to 80 frames a second, irregularly; `ffmpeg` copied frames
  to keep the 120-a-second film even, and most **repeats** are those copies.
- Of the ticker's 60 frames a second, the film saw 50 to 57. The film cannot say whether the
  3 to 10 frames a second it missed were never painted by Ghostty or painted and missed by the
  camera. The holds suggest the camera: a 60-a-second frame lasts 16.7 ms, two film frames, and
  a camera taking a frame every 12 to 14 ms, unevenly, reads it as one, two or three film
  frames, which is the run of 8.3, 16.7 and 25.0 ms holds above, with a median of 16.7 ms.
- So the camera sees at least 57 frames a second when the terminal paints quickly. It cannot
  promise to see every frame of 60, and a terminal painting at 60 will be seen at about 50 to
  57 a second by it.
- Torn is 0 in every run.

## What it means for Apple's Terminal

`cadence-bounce.md` found that Apple's Terminal paints at most about 22 times a second, and
said the camera, delivering about 70 frames a second, was not the limit. Until now that rested
on an argument. Here the same camera, on the same Mac, filming the same ticker at the same grid
size, sees Ghostty at 50 to 57 a second. The camera is not the limit, so the 22-a-second
ceiling belongs to Apple's Terminal: it is now **seen**, not argued.
