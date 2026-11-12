# Working in this repository

Read `CONTEXT.md` first and use its words. Read `docs/architecture.md` for how the parts join.

- Work one issue at a time. Change only the files the issue names.
- `crates/kernel` is the contract between parts built in parallel. Do not change it. If it
  lacks something, stop and say so on the tracking issue.
- No new dependencies unless the issue lists them.
- Before a pull request: `cargo fmt --all`, `cargo clippy --workspace -- -D warnings`,
  `cargo test --workspace`. All three pass.
- A pull request body starts `Closes #<n>`.
- Apple's Terminal is the reference surface. Anything that opens it goes through
  `rigs/apple-terminal/with-screen.sh`, which lets one test window exist at a time.
  Capture only the test window, never the whole screen.
- The bench can show bytes are consistent with a profile. Only the witness can show
  something works. Never write "verified" for a bench result.
- Say "accepted" for a rate the terminal took and "seen" only for one a film shows.
- `lab/` and `rigs/` hold the throwaway experiments the design came from. Read them for
  working code to port; do not build on them.
- Delete anything temporary you create.
