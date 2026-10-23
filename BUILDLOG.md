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

## Session 001 — 2026-10-23 — Iteration 0 (walking skeleton)

**Goal:** Stand up the build: DAY0 data confirmation, repo scaffolding (build log,
ADR index), a compiling+running walking skeleton (window-less here: headless render
path), first screenshots, first measurement rows.

**Started from:** commit `b45d376` (planning package only; no code, no build log).

**Done:**
- Build-log scaffolding in place per PLAN §16.3: `BUILDLOG.md`, `docs/build/STATUS.yaml`,
  `docs/build/measurements.csv`, `docs/build/shots/`, `docs/adr/INDEX.md`,
  `CONTRIBUTING.md` (resume protocol verbatim).
- **DAY0 §1 smoke test PASSED**: ESA TAP sync TOP-10 in cone(83.82,−5.39,0.5°), 1.2 s,
  brightest G=4.96 (Alnilam at the cone centre — pointing verified).
- **DAY0 §2 count MEASURED**: cone(83.82,−5.39,20°) G<20 = **15,573,949 rows**
  (async job `b3a87cff-…-O`; PLAN estimated 35–50 M → ADR-0004 records the correction;
  raw ≈ 6 GB, baked ≈ 0.5 GB).
- **DAY0 §2 bright subset FETCHED + COMMITTED**: `etl/norm/orion_bright_g10_20k.csv`
  (17,494 rows, G<10, ordered by mag). The It.0 skeleton renders these REAL stars —
  10,000 used, 123 skipped for null parallax (incl. Rigel-class bright stars; see
  "Not done").
- Cargo workspace per PLAN §4.1: `galaxia-core` (unit newtypes §5.1 [MUST],
  ICRS→cartesian, crude bp_rp colour), `galaxia-render` (GpuContext, frame-graph
  skeleton w/ 7 named pass slots, star pass (32 B/star instanced billboards, additive),
  WGSL hot-reload watcher (validate-before-swap), GPU timestamp timers, offscreen PNG
  target), `galaxia-app` (clap CLI, winit event loop + egui overlay (windowed),
  headless `--screenshot/--bench/--frames` path, camera w/ exponential speed + damped
  rotation, input→action map, star CSV loader w/ provenance + NaN policy).
- wgpu **pinned =30.0.1**; API drift vs training data resolved against registry
  sources (wgpu 30: TexelCopy* renames, CurrentSurfaceTexture enum, Queue::present,
  fallible get_mapped_range, multiview_mask, immediate_size, split timestamp
  features; egui 0.37 run_ui; glam 0.34 dropped look_at_* and f32
  perspective_infinite_reverse_rh → hand-rolled reversed-Z infinite-far (§5.4)).
- **ADR-0010**: sandbox (1 GB RAM) cannot compile `ash`; backend split behind cargo
  features (`vulkan` default for the Windows target, `gles` for the sandbox build:
  `cargo build -p galaxia-app --no-default-features --features gles`).
- First renders: `docs/build/shots/2026-10-23-iter0-orion-wide.png` (2560×1440, real
  Gaia stars, cone boundary visible), `…-orion-belt.png`.
- Tests: 11/11 green (units, math incl. hand-verified Alnilam–Betelgeuse 9.82°,
  colour clamps/NaN, procedural determinism+bounds).

**Measured:**
| metric | before | after | budget |
|---|---|---|---|
| Orion cone rows G<20 (est→measured) | 35–50 M (plan) | **15.57 M** | — (ADR-0004) |
| TAP smoke latency | — | **1.2 s** | n/a ✔ |
| stars rendered (real Gaia, orion_wide) | 0 | **10,000** | 10,000 (It.0) ✔ |
| headless frame (llvmpipe, 2560×1440, CPU wall) | — | **≈20–46 ms** | n/a (sandbox ≠ Titan) |
| GPU star-pass ms (llvmpipe timestamps) | — | **untrustworthy (<0.01 ms)** | 5.5 ms on Titan (It.2) ✘ not measurable here |
| tests | 0 | **11 passed** | green ✔ |

**Blockers hit:**
- apt mirror (aliyun) throttled to ~32 KB/s → 900 s timeout; switched to
  deb.debian.org (81 MB/s) — full toolchain install then took 22 s. Dead end recorded.
- rustc SIGKILL (OOM) on `ash` at 1 GB RAM even with debug=0 → ADR-0010 GLES split.
  Also: egui-wgpu default features and member-crate defaults silently re-enable
  `wgpu/vulkan`; `cargo tree -e features -i ash` is the tool that found both.
- astroquery async poller never observed COMPLETED for the count job (phase endpoint
  said COMPLETED) → dead end; plain-REST fetcher (`etl/day0_rest.py`) replaced it.
- Gaia returns results **gzip-compressed** and async results as **VOTable** despite
  FORMAT=csv on job submit → both handled in day0_rest.py.
- wgpu map_async-before-submit race: mapping is tied to the submission index at call
  time → "buffer is still mapped" validation error. resolve() now encodes only;
  begin_readback() maps after submit.
- `external.gaiaedr3_distance` on the archive has 2018-era columns (r_med_geo,
  r_med_photogeo) and **0 rows** for our 190 no-parallax source_ids → Bailer-Jones
  distances for bright stars unresolved (see Not done).

**Not done / discovered work:**
- **190 brightest cone stars have null parallax** (Rigel G=2.08 among them) and no
  distance in `external.gaiaedr3_distance` → skipped by the loader (never invent
  data). It.1 ETL must find the correct DR3 Bailer-Jones access path or a literature
  fallback (HIP/GCVS) with provenance flag "literature".
- GPU timer readback is single-buffered → blocks (`poll_results` waits). Must be
  double-buffered in It.2 (comment in timing.rs says so).
- Windowed path (winit+egui overlay) compiles but is untested (no display here).
- Orion full-cone streaming downloader `etl/fetch_orion.py` NOT written yet
  (DAY0 §3); count says it is ~6 GB raw — feasible streamed even in the sandbox.
- Pleiades fallback fixture (DAY0 §4) not fetched.
- `lit_pixels` content check threshold is defeated by the dark background encoding;
  needs a real threshold or a difference-vs-blank-frame check (It.1 golden harness).

**Screenshot:** `docs/build/shots/2026-10-23-iter0-orion-wide.png`

**Next (precise):** Write `etl/fetch_orion.py` per `templates/DAY0-orion-fetch.md` §3
(streaming-partitioned, HEALPix Nside=32, free-disk gate ≥40 GB, resumable manifest,
mirror failover), then run it for a **5-partition smoke slice** in the sandbox
(proves fetch→delete-raw cycle under the 9 GB disk), and append the manifest to
`etl/reports/fetch_manifest.json`. Do NOT start the full cone download until the
slice round-trips and the quantise stage (32 B/star, PLAN §8.4) exists to consume it.

**Commit:** `869f9b9`

---

---

## Session 001 addendum — 2026-10-23 — Iteration 0/1 boundary (DAY0 §3 fetcher)

**Goal:** close the DAY0 §3 gap: streaming-partitioned Orion downloader, proven in the
sandbox under the 9 GB disk envelope before anyone trusts it overnight on the target.

**Started from:** commit `953205c` (session 001 recorded).

**Done:**
- `etl/fetch_orion.py`: HEALPix Nside=32 cone partitioning (424 pixels measured vs
  500–550 estimated), per-pixel ADQL POLYGON (interpolated great-circle edges) ANDed
  with the cone circle, free-disk gate, mirror failover (ESA→ARI→CDS→NADC), resumable
  manifest, raw-deleted-after-bake (§8.4.1 rule 1), gzip-aware sync TAP.
- `etl/galaxia_quant.py`: 32 B/star record v0 (xyz f32 pc, mag f32, rgb f32, flags u32;
  bit0 provenance, bit1 has-parallax). Final §8.4 schema = It.1.
- **Smoke slice PASSED** (5 partitions): 12,220 rows baked = rows×32 B exactly; raw CSV
  deleted; manifest `etl/reports/fetch_manifest.json` records mirror/rows/sha256/timing.
  Skip fraction ≈ 30 % at G<20 (null/negative parallaxes at the faint end) — expected,
  counted, never invented.
- Guard added: a partition returning > 400 k rows aborts (polygon-winding canary).
- **Full 424-partition run launched unattended** at 09:41 UTC in the sandbox
  (`--min-free-gb 1` override; target keeps the 40 GB gate). ~7 s/partition observed →
  ≈ 50 min. Resumable: killed partitions simply re-run; manifest skips baked ones.

**Measured:**
| metric | before | after | budget |
|---|---|---|---|
| partition wall time (sync TAP, ESA) | — | **6–7 s** | n/a ✔ |
| rows/partition (cone-interior pixel) | est 70–90 k | **15–24 k** | — (fainter than §16.2 guess) |
| baked bytes vs rows×32 | — | **exact** | 32 B/star ✔ |
| raw residency per partition | ~30 MB | **deleted in-process** | peak rule ✔ |

**Blockers hit:**
- healpy `boundaries()` returns (3, N) → transpose; `vec2ang` on single vectors returns
  arrays → lon/lat computed by hand. Both one-line fixes, recorded so nobody re-debug.

**Not done / discovered work:**
- Interior-pixel row counts (15–24 k) are ~4× below the DAY0 estimate of 70–90 k,
  consistent with the 15.57 M total; reinforces ADR-0004.
- Sync TAP per partition is fine at Nside=32 but will not scale to all-sky tiers;
  It.1 ETL moves to async jobs + parallel workers (DAY0 §3 notes).
- Quantised v0 has no Morton order / LOD metadata — octree builder (It.1) consumes
  these .bin partitions and emits the §8.4 artifact.

**Screenshot:** n/a (data-path session; visual unchanged from
`docs/build/shots/2026-10-23-iter0-orion-wide.png`).

**Next (precise):** when `etl/reports/fetch_manifest.json` shows 424 partitions (or the
run was killed: re-launch `python3 etl/fetch_orion.py --min-free-gb 1`), verify
total_rows ≈ 15.57 M × (1 − skip) and total baked ≈ rows×32 B; then start It.1:
`crates/galaxia-data` octree builder reading `etl/build/*.bin` → Morton sort →
leaf packs (~10⁵ stars/leaf) → Rust async loader + LRU resident set (PLAN §16.4 It.1).

**Commit:** `abdf5de`
