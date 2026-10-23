# CONTRIBUTING — build-session operating rules

This project is built across many sessions, possibly months apart. The build log
is part of the deliverable, not documentation about it (PLAN §16.3).

## ⟲ RESUME PROTOCOL — execute in order, skip nothing

1. Read [`docs/build/STATUS.yaml`](docs/build/STATUS.yaml) → *where the project is*
2. Read the **last three sessions** in [`BUILDLOG.md`](BUILDLOG.md) → *what was just happening*
3. Read [`docs/adr/INDEX.md`](docs/adr/INDEX.md); skim any ADR touched in those sessions → *why it is that way*
4. `git log --oneline -20` and `git status` → *is the tree clean?*
5. `cargo build && cargo test && cargo run -- --scene orion_wide`
   Confirm it runs **and looks like the last image in `docs/build/shots/`**
   (Sandbox builds: `cargo build -p galaxia-app --no-default-features --features gles`
   then `./target/debug/galaxia --scene orion_wide --screenshot /tmp/check.png` — ADR-0010.)
6. `cargo run -- --bench` → compare against `measurements_latest` in STATUS.yaml.
   **If any number moved > 10 %, find out why BEFORE adding features.**
7. Only now: start on `next_action`.

> **Trust nothing that is not verified by running it.** The log is a map, not the
> territory. If step 5 or 6 fails, the session's goal is to fix that — not to build
> new things on a broken base. Update STATUS.yaml to say so.

## Session end order (non-negotiable)

`cargo test` green → screenshot → `docs/build/measurements.csv` row →
`docs/build/STATUS.yaml` rewritten → entry appended to `BUILDLOG.md` → **commit last**,
so the hash in the entry is real. Push after every file change.

## Always-working rules (PLAN §16.6)

- `main` is always runnable. No long-lived branches.
- CI on every commit: build + clippy -D warnings + test + precision torture +
  determinism + golden images (CI itself lands by It.3; until then the build
  session performs these steps by hand and records them).
- A red main stops all other work.
- No feature is "done" until it has a measured number attached.
- Numbers, not adjectives. "11.2 ms → 4.1 ms against a 5.5 ms budget" is a record;
  "faster" is worthless.
- Record failures and dead ends — they are the most valuable content in the log.
- Every deviation from PLAN.md gets an ADR: context / decision / consequences.
- Never invent data. NaN/unknown is valid and must render as "unknown".
- No f64 in WGSL (CI lint lands It.3; enforce by review until then).
- Never block the render thread on I/O (the It.0 timer-readback serialization is
  recorded debt in timing.rs, to be double-buffered in It.2).
