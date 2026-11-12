# Cadence of the ticker in Apple's Terminal 2.15, filmed

macOS 27.2, Apple M1 Max, built-in 3456 × 2234 display (1728 × 1117 points), Menlo 12,
160 × 45 cells. Filmed 2026-11-11 with `witness-film` through
`rigs/apple-terminal/with-screen.sh`:

```
witness-film --font 12 --cols 160 --rows 45 --seconds 10 -- witness-ticker --seconds 30 --cols 160
```

`witness-ticker` paints the frame counter strip and a bar moving one cell a frame, 60 times
a second, with plain writes. Every number below is **seen**: read from a film of the window.

## The film

| | |
|---|---|
| Film rate asked of ffmpeg | 120 frames a second |
| Film rate the screen recording delivered | 75.0 frames a second; ffmpeg copied frames to keep 120 even |
| Film frames read | 1,180 (9.83 s) |
| Strip found at | x 20, y 78, 252 × 30 pixels: cells 14 pixels wide, 30 tall |

## What the film shows

| | |
|---|---|
| Frames the ticker sent in that time | 590 (60 a second) |
| Frames shown | 213: **21.7 a second seen** |
| Frames skipped | 377 |
| Torn | 0 |
| Longest hold | 66.7 ms |
| Median hold | 41.7 ms; mean 46.2 ms |

How long each frame shown stayed on screen:

| Hold | Frames |
|---|---|
| 8.3 ms | 1 |
| 16.7 ms | 1 |
| 25.0 ms | 3 |
| 33.3 ms | 45 |
| 41.7 ms | 60 |
| 50.0 ms | 46 |
| 58.3 ms | 44 |
| 66.7 ms | 13 |

## What it means

Apple's Terminal took the ticker's frames at 60 a second (accepted), but the film shows it
put about 22 a second on the screen. The rest were never seen: roughly two frames in three
were skipped, none torn. The screen recording delivered frames faster than the strip
changed, so the limit is the terminal's painting, not the film. A second film of 5.8 s
showed the same: 127 shown, 222 skipped, 0 torn.

Holds are counted in film frames of 8.3 ms, so each is known to within one film frame.
