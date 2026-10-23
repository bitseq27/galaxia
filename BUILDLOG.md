# Build Log — PROJECT MILKY WAY (`galaxia`)

**Append-only.** Never edit or delete an entry. Corrections are new entries.
A session that does not end with an entry here **did not finish**.
Specification: `PLAN.md` §16.3.

---

## ⟲ RESUMING WORK? Do these in order. Skip nothing.

1. Read [`docs/build/STATUS.yaml`](docs/build/STATUS.yaml) → *where the project is*
2. Read the **last three sessions** below → *what was just happening*
3. Read [`docs/adr/INDEX.md`](docs/adr/INDEX.md); skim any ADR touched in those sessions → *why it is that way*
4. `git log --oneline -20` and `git status` → *is the tree clean?*
5. `cargo build && cargo test && cargo run -- --scene orion_wide`
   Confirm it runs **and looks like the last image in `docs/build/shots/`**
6. `cargo run -- --bench` → compare against `measurements_latest` in STATUS.yaml.
   **If any number moved > 10 %, find out why BEFORE adding features.**
7. Only now: start on `next_action`.

> **Trust nothing that is not verified by running it.** The log is a map, not the
> territory. If step 5 or 6 fails, the session's goal is to fix that — not to build
> new things on a broken base. Update STATUS.yaml to say so.

---

## Entry template — copy this for every session

```markdown
## Session NNN — YYYY-MM-DD — Iteration N (<name>)

**Goal:** <one or two sentences: what "done" looks like for this session>

**Started from:** commit `<hash>`. <one sentence on the state you inherited>

**Done:**
- <what changed, with file paths — specific enough to find it again>

**Measured:**
| metric | before | after | budget |
|---|---|---|---|
| <e.g. 10M stars @1440p> | <ms> | **<ms>** | <ms> ✔/✘ |

**Blockers hit:**
- <what broke, what you tried, what resolved it. Link the ADR if you deviated.>
- <Dead ends are the most valuable content in this log. Record them.>

**Not done / discovered work:**
- <what you found that needs doing but did not do>

**Screenshot:** `docs/build/shots/YYYY-MM-DD-iterN-<what>.png`

**Next (precise):** <the specific next action — specific enough that a cold reader
can start without re-deriving any context. This is the most important line here.>

**Commit:** `<hash>`
```

### Rules

- **Numbers, not adjectives.** "Faster" is worthless. "11.2 ms → 4.1 ms against a 5.5 ms budget" is a record.
- **Record failures and dead ends.** A session that discovered something doesn't work was not wasted — *provided it wrote it down*.
- **Every entry ends with a precise `Next`.** Not "continue work on volumetrics."
- **Link commits and ADRs.** This log indexes the repo history; it does not replace it.
- **Screenshot every iteration.** The visual record is how you catch regressions that numbers miss.
- **Session end order:** `cargo test` green → screenshot → `measurements.csv` row → `STATUS.yaml` rewritten → entry appended here → **commit last**, so the hash in the entry is real.

---

## Session 001 — <date> — Iteration 0 (walking skeleton)

**Goal:** *(the first entry writes itself — see `PLAN.md` §16.4, Iteration 0)*

---
