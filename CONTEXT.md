# Phosphor: the language

Phosphor puts the best picture a screen can carry onto it, keeps it moving smoothly, and
proves it on the real screen. Use these words, with these meanings, in code, issues and
pull requests. If a word is missing, propose it on the tracking issue; do not invent a
synonym.

## The parts

| Part | Its job | Crate |
|---|---|---|
| **Composition** | What should be seen, and how it changes. Knows nothing about surfaces. | `compose` |
| **Foundry** | Turns fonts and images into dots ahead of time. | `foundry` |
| **Presentation** | The core. The best picture this surface can carry on this frame, inside its budget. | `fit`, `encode`, `govern` |
| **Surface** | One real screen, known truthfully. The only part that touches a terminal. | `surface` |
| **Witness** | What was really shown, by photograph and film. | `witness` |
| **Bench** | The pretend run: same input, same bytes, on any machine. | `bench` |
| **Kernel** | The few types all parts share. | `kernel` |
| **Apps** | Things drawn with Phosphor. Not part of it. | `apps/*` |

## Words

**Surface.** A screen Phosphor draws to. Apple's Terminal is the reference surface.

**Cell.** One character position on a surface. It shows one character in two colours.

**Ink and paper.** A cell's two colours. Ink draws the character's shape; paper is behind it.

**Ink rule.** A cell has only one ink and one paper. Any picture has to live within that.

**Dot.** The smallest piece of picture Phosphor controls. Block characters split a cell
into dots.

**Dot ladder.** The ways to split a cell, coarsest first: half (1 × 2 dots), quarter
(2 × 2), sixth (2 × 3), eighth (2 × 4). A surface draws only some rungs. On the half rung
each dot takes one of the cell's two colours, so the ink rule costs nothing.

**Canvas.** A picture made of dots.

**Layer, placement, motion.** A layer is a canvas with a position. A placement is where it
sits, in dots. A motion says how a placement changes over time.

**Grid.** What a surface shows: its cells.

**Fit.** Turning a canvas into a grid for one rung of the ladder.

**Picture meant.** The dots a grid should show. **Picture seen.** The dots read back from
a photograph of the real window.

**Damage.** The cells that differ between the grid meant now and the grid last sent.

**Run.** A stretch of cells painted without changing colour. Each new run costs a colour
change. On Apple's Terminal, runs are what cost time; bytes and cells are nearly free.

**Tally.** What one frame contains, counted: bytes, cells, runs, and crowding (runs per
row, squared, summed).

**Price list.** What a surface charges for each thing in a tally, in its own time,
measured on the real surface.

**Budget.** The time a surface has for one frame: 1 / the beat. The beat never runs faster
than the surface's paint rate, so the budget is never shorter than one paint.

**Cost.** The price of a tally under a price list. A frame fits when cost ≤ budget.

**Governor.** The part that keeps every frame inside the budget. It chooses the rung, the
grid size, how thrifty to be, and what to put off.

**Thrift.** Simplifying a canvas's colours so its grid needs fewer runs.

**Developing.** Still detail arriving over several frames while motion stays on the beat.

**Beat.** The even timing of frames. Unless asked otherwise, the beat is the surface's paint
rate.

**Paint rate.** How often a surface is seen to paint; measured on film, never taken from acks.
A frame sent faster than this is read and then painted over before it is ever seen.

**Frame.** One grid, sent whole.

**Ack.** A surface's answer that it has read a frame. It does not mean the frame was painted.

**Capability.** A behaviour the witness has seen work on a surface. Unproven means unusable.

**Quirk.** A behaviour a surface gets wrong.

**Profile.** Everything known about one surface: its rungs, colour depth, capabilities and
price list.

**Probe.** A question put to a surface at start-up to fill in its profile.

**Fidelity.** How closely the picture seen matches the picture meant.

**Cadence.** When each frame really appeared, from a film of the window.

**Accepted.** The surface took frames at a rate. **Seen.** The film shows them at that
rate. Only *seen* may be called smooth.

**Typeface, glyph sheet, bake.** A typeface is a font. Baking turns it into a glyph sheet:
each letter as dots at one size.

**Drawn text** is made of dots and moves by dots. **Native text** is the terminal's own
letters: small, sharp, and locked to cells.

## Said another way

| If you hear | It means |
|---|---|
| "characters changing" | cells in the damage |
| "a byte limit for 60 frames a second" | the budget, priced with the surface's price list |
| "indexed colours" | a colour depth of 256 or 8 |
| "compressing the data" | thrift (fewer runs) and encoding (fewer bytes) |
| "it looks stuttery" | poor cadence |
| "it looks glitchy" | poor fidelity |

## Rules that always hold

1. A capability is something the witness has seen work on that surface. A passing bench is
   not proof.
2. The governor never sends a surface a frame that costs more than its budget.
3. Motion outranks detail.
4. A surface's quirks live in its profile. Composition never hears about them.
5. Only the surface part touches a terminal. Everything else is plain calculation.
6. Say *accepted* until a film shows it; then say *seen*.
