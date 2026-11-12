# Fidelity of the benchmark in Apple's Terminal 2.15, photographed

macOS 27.2, Apple M1 Max, built-in Retina display, Menlo, 160 × 45 cells. Photographed
2026-11-11 with `witness-bounce` through `rigs/apple-terminal/with-screen.sh`:

```
cargo build -p phosphor -p phosphor-witness
rigs/apple-terminal/with-screen.sh target/debug/witness-bounce --all docs/evidence/bounce
```

For each combination `witness-bounce` works out the **picture meant** on its own: the
benchmark composed at the freeze time, fitted to the rung, then `unfit` back to dots. It then
opens one test window (line spacing from `witness::still::line_spacing`, so a cell is one em
tall), runs

```
phosphor play bounce --profile apple --rung <rung> --marks --freeze-at <t> --seconds <t + 60>
```

waits until two seconds after the freeze, photographs only that window, closes it, reads
the **picture seen** from between the registration marks, and compares the two. A dot is
wrong when any channel differs by more than 24. The four corner cells, where the marks sit,
are left out.

**Control** is the number of wrong dots against the picture meant one second *later*. It
shows the comparison can tell one picture of the benchmark from another; it is not a
result.

## Results

| Rung | Font | Freeze (s) | Dots | Wrong | Worst | First wrong | Control | Photograph |
|---|---|---|---|---|---|---|---|---|
| half | 12 | 0.5 | 14392 | 0 | 1 | – | 5029 | [half-font12-t0.5.png](../evidence/bounce/half-font12-t0.5.png) |
| half | 12 | 3.0 | 14392 | 0 | 1 | – | 4827 | [half-font12-t3.0.png](../evidence/bounce/half-font12-t3.0.png) |
| half | 12 | 7.3 | 14392 | 0 | 1 | – | 6816 | [half-font12-t7.3.png](../evidence/bounce/half-font12-t7.3.png) |
| half | 6 | 0.5 | 14392 | 0 | 1 | – | 5029 | [half-font6-t0.5.png](../evidence/bounce/half-font6-t0.5.png) |
| half | 6 | 3.0 | 14392 | 0 | 1 | – | 4827 | [half-font6-t3.0.png](../evidence/bounce/half-font6-t3.0.png) |
| half | 6 | 7.3 | 14392 | 0 | 1 | – | 6816 | [half-font6-t7.3.png](../evidence/bounce/half-font6-t7.3.png) |
| quarter | 12 | 0.5 | 28784 | 0 | 1 | – | 8582 | [quarter-font12-t0.5.png](../evidence/bounce/quarter-font12-t0.5.png) |
| quarter | 12 | 3.0 | 28784 | 0 | 1 | – | 8289 | [quarter-font12-t3.0.png](../evidence/bounce/quarter-font12-t3.0.png) |
| quarter | 12 | 7.3 | 28784 | 0 | 1 | – | 14306 | [quarter-font12-t7.3.png](../evidence/bounce/quarter-font12-t7.3.png) |
| quarter | 6 | 0.5 | 28784 | 0 | 5 | – | 8582 | [quarter-font6-t0.5.png](../evidence/bounce/quarter-font6-t0.5.png) |
| quarter | 6 | 3.0 | 28784 | 0 | 5 | – | 8289 | [quarter-font6-t3.0.png](../evidence/bounce/quarter-font6-t3.0.png) |
| quarter | 6 | 7.3 | 28784 | 0 | 5 | – | 14306 | [quarter-font6-t7.3.png](../evidence/bounce/quarter-font6-t7.3.png) |

All twelve show the picture meant: 0 wrong dots. The largest difference in any channel of
any dot is 5 (out of 255). The same twelve were photographed three times, before and after
the benchmark moved to drawn text (#23), with the same numbers each time.

At 160 × 45 the computer's screen is narrower than the 400 dots the baked JetBrains Mono
headline needs, so these photographs show the headline in the small 5 × 7 face. The baked
headline is not covered by this table.

## What the dot count does not see: seams at the cell's right edge

The witness reads the middle half of each dot, so anything that happens only at a cell's
edge is not counted. The photographs show one such thing. In a cell with two colours, the
ink of the block character does not end exactly at the cell's edge:

| Font | Cell (pixels) | Menlo's block (pixels) | Seen |
|---|---|---|---|
| 12 | 14 × 24 | about 14.45 wide | ink spills under half a pixel into the next cell; faint |
| 6 | 8 × 12 | about 7.2 wide | the cell's last pixel column is part paper: a visible line |

Terminal makes a cell a whole number of points wide (7 at font 12, 4 at font 6) but draws
the block from the font, whose advance is 0.602 em (7.22 and 3.61 points). In the font 6,
quarter, 7.3-second photograph, 1,310 of 13,588 half-cells checked have an outermost
pixel column more than 24 away from their dot, all on the right. At font 12 there are 343,
on both sides. Setting `FontAntialias` to false in the window's settings did not change it.

No choice of character, ink or paper avoids it: in every two-colour cell the ink touches
the cell's edge. It is a quirk of the surface at font sizes whose cell width is not Menlo's
advance, not a wrong dot, and it is recorded here, not fixed.

## Defects found

- `witness::still::compare` assumed the half rung when leaving out the corner cells, so on
  the quarter rung the second column of each mark would have counted as wrong. It now takes
  the rung (`compare_skips_quarter_rung_corners`).

No defect was found in composition, fit, encode or the program: the bytes `phosphor play
bounce` sent were shown exactly, on both rungs, at both sizes, at all three times.
