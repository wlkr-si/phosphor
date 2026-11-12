# Phosphor: the language

Phosphor is a set of animations that run in a terminal, built on
[OpenTUI](https://github.com/anomalyco/opentui). Use these words, with these meanings, in
code, issues and pull requests.

## Words

**App.** One animation you can run on its own, such as the screensaver. Each app lives in
`apps/<name>/` and builds into its own program file.

**Program file.** An app built into one file that runs without installing anything else:
`phosphor-<app>-<computer type>`, for example `phosphor-dashboard-linux-x64`.

**Shared pieces.** Code more than one app can use: colours, big letters, the pixel
alphabet, rectangles drawn as cells, and the screensaver. They live in `src/`.

**Dashboard.** The first app: a still retro computer filling the window, its screen
showing cards, with a screensaver that takes over when it is left alone.

**Screensaver.** A full-window animation of a big title, made of **effects** (rain,
decrypt, beams, swarm) that each run ten seconds. It starts after `idle` seconds with no
mouse or keys, runs for `screensaver` seconds, and stops the moment someone moves the
mouse, clicks or presses a key. A **wipe** is the bright line that sweeps between the two.

**Computer.** The drawing: a beige monitor on a desktop case with vents and two drive
slots. Its **screen** is the dark area inside the monitor; its **label** is the text on
the monitor's chin.

**Card.** One bordered box on the screen with a title, such as Workhorse or Health. A card
has a **type**, which decides how it draws, and a **source**, which says where its data
comes from.

**Card types.** machine, list, progress, checks, dots and text. `apps/dashboard/data.ts`
defines the data each one draws.

**Source.** Where a card's data comes from: demo (made-up data that moves), system (this
computer's own numbers), url, file or command. Cards with the same source share one
fetch; `path` picks each card's part.

**Layout.** Rows of columns of cards. Heights and widths are shares, not cells.

**Settings file.** A JSON file that chooses the title, label, colours, speed, quiet hours,
cards and layout. Without one the screensaver shows the example.

**Quiet hours.** A time range when the screen goes dark, such as 23:00-07:00.

**Example.** The settings and demo data the public app ships with. Real people's machines,
addresses and job names never go in this repository.

## Rules that always hold

1. Check every change in Apple's Terminal and in a terminal that draws block characters
   itself, such as Ghostty. They differ, and both have to look right.
2. Nothing large moves. Text sits on whole characters, so a big shape gliding slowly
   always looks like steps. Animation is many small pieces moving a cell at a time,
   quickly, or cells changing colour where they stand.
3. Solid areas are coloured spaces. Apple's Terminal draws block characters from the font:
   `█` comes out smaller than a cell, and partial blocks leave a hairline.
4. The repository is public. Private data lives in private settings files elsewhere.
