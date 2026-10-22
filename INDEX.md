# PROJECT MILKY WAY (`galaxia`) — Planning Package

Planning artifacts for a Rust/WGSL Windows application rendering a navigable,
science-faithful, live simulation of the Milky Way. **No code has been written.**
This package exists to instruct the build in a separate session.

Prepared 2026-10-23 · Plan version **v1.2**

---

## Files

| File | What it is | When to use it |
|---|---|---|
| **`PLAN.md`** | The authoritative specification. ~28k words, 19 sections + 5 appendices. Contains the decision log, the three hard problems, full subsystem specs, the tiered data-source answer, the build plan, and the risk register. | Hand to the build session. Read §1, §1.1, §2, §3, §8.4.1, §16 first. |
| **`BUILD_BRIEF.md`** | Condensed primer — `PLAN.md` Appendix A extracted, plus the disk constraint, Orion region, build philosophy, iteration ladder, and build-log rules. | **Paste this as the opening context of a fresh session**, then hand over `PLAN.md`. |
| `templates/BUILDLOG.md` | The append-only build log, with the resume protocol at the top and a per-session entry template. | Copy to repo root at Iteration 0. Fill in Session 001 *before* writing any Rust. |
| `templates/STATUS.yaml` | The machine-readable resume file, fully commented. | Copy to `docs/build/STATUS.yaml`. Rewrite at the end of **every** session. |
| `templates/measurements.csv` | Header row for the measurement log. | Copy to `docs/build/measurements.csv`. One row per (session × benchmark scene). |
| `templates/adr-INDEX.md` | ADR index with template and nine pre-allocated decision records. | Copy to `docs/adr/INDEX.md`. |
| `templates/DAY0-orion-fetch.md` | The day-0 Gaia data fetch: pre-flight, smoke test, row count, streaming-partitioned downloader spec, the Pleiades fallback fixture, and the licence attribution. | **Do this first, before any application code.** It runs unattended overnight. |

---

## The decisions, in one place

Locked by the product owner on 2026-10-23. Full rationale and consequences in `PLAN.md` §1.

| | |
|---|---|
| **Purpose** | Personal / learning project. Never distributed. |
| **Stars** | Hybrid — real Gaia DR3 (→ DR4 in Jan 2027) where available, Besançon-style procedural synthesis beyond, with a provenance flag on every object. |
| **Zoom depth** | Galactic (200 kpc) → planetary orbit (10⁴ km). **No surface landing.** |
| **Solar System** | Keplerian + perturbations (VSOP87/ELP2000). **No SPICE.** Arcminute target. |
| **Stack** | wgpu (Vulkan) + custom renderer + custom frame graph. **No Bevy.** Python for the offline ETL; Rust reads baked binary. |
| **Art direction** | Science-faithful. Beauty emerges from physics. Nothing invented. |
| **UI** | Diegetic 3D — *deferred to wave W2; the prototype uses egui (ADR-0003).* |
| **Sequencing** | **Prototype first, then iterate.** Walking skeleton by day 2. |
| **Scripting / cinematics / networking / VR** | None. |

## The target machine

| | |
|---|---|
| **GPU** | NVIDIA TITAN RTX — Turing TU102, 4 608 CUDA cores, 24 GB GDDR6, 672 GB/s, 16.3 TFLOPS FP32, **6 MB L2** |
| **Host** | 96 GB RAM, 32 threads, **200 GB free NVMe** |
| **Plan of record** | **4K60 output via a 2560×1440 internal buffer + FSR2**, 8–10 M visible stars. Native 4K60 with 10 M+ stars is *not* achievable on this card. GPU budget 13.0 ms. |
| **Disk reality** | 200 GB is the tightest resource in the project. The full Gaia CSV dump (~1 TB) cannot be downloaded. **The ETL must be streaming-partitioned** (`PLAN.md` §8.4.1). Tier 3 (300 GB) is not installable here. |

## Prototype region

**Orion.** Cone centre α = 83.82°, δ = −5.39° (ICRS), radius 20°, `phot_g_mean_mag < 20.0`
→ ~35–50 M rows, ~14–20 GB raw CSV, **~1.1–1.6 GB baked**. Chosen because it contains the
whole Orion Molecular Cloud Complex (M42/M43, the Trapezium and θ¹ Ori C, M78, NGC 1977,
Barnard's Loop, the Flame, IC 434/Horsehead, LDN 1630), Betelgeuse/Rigel/the Belt, open
clusters with real member lists, extreme density contrast, and it sits entirely inside the
Edenhofer 1.25 kpc 3D dust map. Details: `PLAN.md` §16.2.

## Timeline

| | Full-time | 20 h/wk | With an AI pair |
|---|---|---|---|
| **Walking skeleton** (10 k hardcoded point stars, flyable camera, debug overlay) | **2 days** | 4–5 days | 1–2 days |
| **Working prototype** (7 iterations, `PLAN.md` §16.4) | **5–6.5 weeks** | 12–14 weeks | 4–5 weeks |
| **Post-prototype waves W1–W8** | ~40 weeks | — | — |

---

## Two things to know before starting

**1. The build log is part of the deliverable, not documentation about it.** This project
will be built across many sessions, possibly months apart. `PLAN.md` §16.3 specifies three
files (`BUILDLOG.md` append-only, `docs/build/STATUS.yaml` rewritten every session,
`docs/build/measurements.csv` append-only) plus a resume protocol. **A session that does
not update them did not finish.** Templates are in `templates/`.

**2. One thing cannot be deferred.** The precision architecture (`PLAN.md` §H1 / §5 / §16.4
Iteration 3) is the only piece that cannot be retrofitted cheaply, because every transform
in the codebase depends on it. The walking skeleton is allowed a single-frame f64 shortcut
that covers Orion — **but it must be retired in Iteration 3, not allowed to become
permanent.** Everything else can be reordered; this cannot.

---

## Immediately actionable

1. `templates/DAY0-orion-fetch.md` §1 — smoke-test the Gaia TAP endpoint (2 minutes).
2. §2 — count the rows (1 minute).
3. §3 — start the streaming-partitioned Orion download. It runs unattended overnight.
4. §4 — build the tiny Pleiades fallback fixture so a failed download never blocks the build twice.
5. §5 — while it downloads: `cargo init`, pin `wgpu`, window + triangle, **WGSL hot-reload**,
   GPU timing query sets, and copy the build-log templates into place.
