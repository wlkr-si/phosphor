# Working in this repository

Read `CONTEXT.md` first and use its words.

- Apps live in `apps/<name>/` with a `main.ts`. Code more than one app uses lives in `src/`.
- No new dependencies unless the issue lists them. OpenTUI is the only runtime dependency.
- Before a pull request: `bun run check` (type check and tests) passes.
- A pull request body starts `Closes #<n>` when it closes an issue.
- Look at your change in Apple's Terminal before calling it done:
  `scripts/shot.sh shots/name.png` opens one test window, photographs only that window and
  closes it. One test window at a time; never capture the whole screen.
- `bun scripts/build.ts` builds program files into `dist/`. Run
  `bun install --os='*' --cpu='*'` once first so every computer type's native part is there.
- This repository is public. Never commit real machine names, addresses, tokens or job
  names; use the example and demo data.
- Delete anything temporary you create, including `shots/` and `dist/`.
