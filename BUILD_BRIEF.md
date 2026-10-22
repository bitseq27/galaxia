# BUILD BRIEF — PROJECT MILKY WAY (`galaxia`)

> **Condensed context primer.** The authoritative specification is `PLAN.md` in this
> directory (v1.2, ~27k words). Read this first, then read `PLAN.md` **§1** (decisions),
> **§1.1** (target hardware — read carefully), **§2** (non-goals), **§3** (hard problems),
> **§8.4.1** (the 200 GB disk constraint), and **§16** (build plan, iteration ladder,
> and the mandatory build log) in full before writing any code.

---

PROJECT: "galaxia" — a Rust/WGSL Windows application rendering a navigable,
science-faithful, live simulation of the Milky Way. Personal project, never distributed.
Full spec in PLAN.md. Read §1 (decisions), §2 (non-goals), §3 (hard problems) first.

LOCKED DECISIONS:
- Hybrid stars: real Gaia DR3 (→DR4 in Jan 2027) where available, Besançon-style
  procedural synthesis beyond, with a provenance flag on every object. Never lie
  about which is which.
- Zoom: galactic (200 kpc) down to planetary orbit (10^4 km). NO surface landing.
- Solar System: Keplerian + perturbations (VSOP87/ELP2000). NO SPICE. Arcminute target.
- Stack: wgpu (Vulkan) + custom renderer + custom frame graph. No Bevy. egui only for
  the settings/debug overlay. Python for the offline data ETL; Rust reads baked binary.
- Art: science-faithful. PBR photometry, Harre-Heller star colours, real 3D dust
  extinction, HDR + auto-exposure + filmic tonemap, c/theta^2 perceptual glare pyramid.
  Nothing invented. A labelled "Enhanced visibility" mode handles faint real structure.
- UI: diegetic 3D. World-anchored GPU SDF labels (budget 64, decluttered), radial
  context menus, in-world info panels, interactive classification legend + filters.
- No scripting, no cinematics, no networking, no VR.
- Target: NVIDIA TITAN RTX (Turing TU102, 4608 CUDA cores, 24GB GDDR6, 672 GB/s,
  16.3 TFLOPS FP32, 6MB L2). Host: 96GB RAM, 32 threads.
  4K60 OUTPUT at 13.0 ms GPU budget, but via a 2560x1440 INTERNAL buffer upscaled
  with FSR2 (DLSS is unavailable through wgpu; XeSS DP4a works on Turing but is
  visibly weaker). NATIVE 4K60 with 10M+ stars is NOT achievable on this card.
  Star budget 8-10M visible. Turing floor: no mesh shaders, no Int64 atomics,
  RT unused, VERIFY VK_EXT_shader_atomic_float and keep the u32 fixed-point
  fallback. Favour shared-memory tile accumulation over global atomics (6MB L2).
  PCIe 3.0 caps host->device at ~15.75 GB/s: upload bulk data ONCE, transform on-device.

THE THREE HARD PROBLEMS (all mitigations are mandatory, see §3):
H1 PRECISION: 20.6 orders of magnitude. f64 canonical storage in a hierarchical frame
   tree; per-frame floating-origin rebasing with hysteresis; split-double (hi+lo f32)
   view transform in WGSL (no f64 in WGSL at all); reversed-Z infinite-far with a
   derived near plane; a per-frame render-frame scale exponent.
H2 DENSITY: Morton-ordered octree, ~10^5 stars/leaf, node metadata (~7MB) permanently
   resident, leaf payloads streamed via LRU. LOD criterion is physical:
   M_brightest < m0 - 5*log10(D/D0). GPU-driven compute selection into indirect
   buffers. SOFTWARE RASTERISATION of stars into an HDR target (Schneegans et al.,
   Eurographics 2025: 22M stars in 8ms at 4K on a 4070 Super) with perspective-correct
   solid angle. ~1GB VRAM for 20M resident stars.
H3 VOLUMETRICS: froxel grid 160x90x64 at 1/4 res, log depth slices, 1/8-rate temporal
   reprojection with blue-noise jitter. Hybrid density = 3D dust maps (Edenhofer 2024
   to 1.25kpc; Dharmawardena 2024 all-sky to 2.8kpc) + per-object analytic volumes
   seeded from real catalog data + procedural detail octaves INSIDE those envelopes only.
   Single scatter with a 6-step shadow cone march (this IS the god rays, no separate
   pass) + 2-octave SH ambient + Beer-Powder. Three emission paths: line emission with
   real Halpha/[OIII]/[SII] ratios, Doppler-beamed synchrotron for jets, thermal dust
   as EXTINCTION not emission. Depth culling in dense media = depth-prepass occlusion +
   per-froxel optical-depth saturation + adaptive density gain.

CLOSED-LOOP GOVERNOR (ship in M1, it is a correctness requirement not polish):
PI controller on GPU frame time, target 12.0ms, slew-limited, with hysteresis.
Degradation order: volumetric temporal rate -> volumetric resolution -> label budget ->
glare pyramid depth -> volumetric steps -> star magnitude threshold m0 (stars LAST).

PHASE 0 = 5 de-risking spikes, 6-9 weeks, each with a numeric pass/fail:
 S1 scale/precision torture test (<0.1px drift over 100kpc -> 10^4 km). DO THIS FIRST.
 S2 star density (10M stars at 1440p in <=5.5ms GPU, 60fps while panning; also
    measure 20M and native-4K costs so the preset table is set from measurement).
 S3 volumetrics (<=2.2ms at 1/4 of 1440p, Orion Nebula with real catalog params).
 S4 HDR chain (20 reference stars within dE2000 < 3 of Harre-Heller;
    FSR2 1440p->4K <=1.2ms on the Titan RTX).
 S5 ETL slice (Gaia DR3 Orion region + Messier + Sharpless + dust -> Tier-0 mini
    dataset; <1 arcsec vs Stellarium for 100 named stars; byte-reproducible).
Then a written go/no-go exit review. If S1 fails, STOP - the architecture is wrong.

KEY DATA SOURCES (Tier 1 = minimum viable; full table in §13 / Appendix C):
Gaia DR3 (CC BY-NC 3.0 IGO) | HYG v3 (names!) | SIMBAD (identity + otype taxonomy)
| NASA Exoplanet Archive (>6000 planets, TAP) | JPL SBDB + MPCORB | Edenhofer 2024 +
Dharmawardena 2024 3D dust (Zenodo 10658339) | USGS/PDS planetary albedo maps
|| Messier + Revised NGC/IC + Caldwell | Sharpless, Barnard, Lynds, vdB, RCW, LBN
| Strasbourg-ESO PN | SNRcat | Harris globulars | Cantat-Gaudin open clusters w/ members
| Solomon GMCs | MWPOT18 or McMillan 2017 | BeSSEL spiral arms | DSS2/2MASS/WISE as HiPS
| HI4PI, Haslam 408MHz, Planck 857GHz, Fermi LAT, ROSAT (survey layers)
|| ATNF pulsars | Open Supernova Catalog | Open GRB Catalog | GWTC | AAVSO VSX + GCVS
| LEDA/HyperLeda | McConnach Local Group | Cosmicflows-4 | CNEOS/Sentry
| Harre & Heller 2021 star colours | MIST/PARSEC isochrones | Pickles + SVO filters
| Bailer-Jones distances (USE THESE, NEVER 1/parallax)

GAIA DR4 LANDS 2 DEC 2026 (~2.8B sources, new data model). Adapter-based ETL from
day 1; no downstream stage may reference a Gaia column name. Migration in wave W8.

HARD DISK CONSTRAINT: only 200 GB free NVMe. The full Gaia CSV dump (~1TB) CANNOT be
downloaded, and neither can an all-sky raw extract at G<20 (~400-600GB). The ETL MUST
be streaming-partitioned: query -> process -> write only the compact quantised output
-> DELETE raw -> next partition. Peak disk <= 60 GB. The ETL itself must check free
space and refuse to start below 40 GB. Baked output is 30-60x smaller than raw
(32 B/star quantised vs ~400 B/star CSV) - the constraint is peak RAW residency.
Tiers: T0 Orion 1.5GB -> T1 Core 15GB -> T2 Standard 55GB. T3 Full (300GB) is NOT
INSTALLABLE here; defer indefinitely. Build each tier only when the previous is in
daily use.

PROTOTYPE REGION (confirmed): ORION. Cone centre RA 83.82 deg, Dec -5.39 deg (ICRS,
just north of Alnilam), radius 20 deg, phot_g_mean_mag < 20.0 -> ~35-50M rows,
~14-20 GB raw CSV, ~1.1-1.6 GB baked. Contains the whole Orion Molecular Cloud
Complex (M42/M43, Trapezium + theta1 Ori C, M78, NGC 1977, Barnard's Loop, the Flame,
IC 434/Horsehead, LDN 1630), Betelgeuse/Rigel/Bellatrix/the Belt, Sirius and Procyon
at the edge, open clusters with real member lists, extreme density contrast, and it
sits entirely inside the Edenhofer 1.25 kpc 3D dust map. Also build a TINY FALLBACK
FIXTURE: the Pleiades alone, 2 deg radius, ~1M stars, ~400 MB - so a failed download
never blocks the build twice.

BUILD PHILOSOPHY (product-owner directive): PROTOTYPE FIRST, THEN ITERATE.
Walking skeleton by end of DAY 2 (10k hardcoded stars as points, camera flies, debug
overlay) so every later change modifies a RUNNING app. Every iteration ends runnable,
measured, screenshotted, and logged. No iteration exceeds 6 working days - cut scope,
do not extend. Depth before breadth. Beauty before rigour, but rigour before scale.
HARD EXCEPTION: the precision architecture (H1) MUST land by Iteration 3 - it is the
only piece that cannot be retrofitted cheaply. The skeleton may use a single
f64-relative-to-Sun frame (fine to ~1 kpc, covers Orion) but that shortcut must be
retired in Iteration 3, not allowed to become permanent.
TWO INTENTIONAL PROTOTYPE SHORTCUTS, both ADR'd and both undone later:
UI is egui not diegetic (saves 2-3 weeks; diegetic is wave W2); data is Orion T0 only.

ITERATION LADDER (each 3-6 days, each ends runnable; full table in PLAN 16.4):
 It.0 (1-2d) walking skeleton: window, 10k hardcoded point stars, camera, overlay
 It.1 (4-6d) REAL DATA: T0 ETL + octree streaming, ~35M real Gaia stars
 It.2 (4-6d) BEAUTIFUL: software rasteriser + Harre-Heller colours + c/theta^2 glare
              pyramid + HDR/exposure/tonemap + FSR2.  Gate: 10M stars @1440p <=5.5ms
 It.3 (3-4d) CORRECT AT SCALE: full precision architecture, retire the skeleton
              shortcut.  Gate: <0.1px drift over 100kpc -> 1e4km.  [HARD GATE]
 It.4 (5-6d) VOLUMETRIC: froxel + hybrid density + shadow-march god rays + line
              emission.  Gate: <=2.2ms at 1/4 of 1440p; M42 recognisable vs real image
 It.5 (4-6d) INTERACTIVE: picking, info panel (egui), ~200-object jump list, time
              dial, VSOP87/ELP ephemeris, light-time tau.  Gate: <1 arcmin vs Horizons
 It.6 (3-4d) GOVERNED + HONEST: adaptive governor from real readback, presets from
              measurement, provenance badges, settings persistence, golden images
TOTAL 24-32 working days = 5-6.5 weeks full-time (~12-14wk at 20h/wk).
Then a WRITTEN exit review (PLAN 16.7) before starting wave W1.

POST-PROTOTYPE WAVES (2-3 week iterations, same discipline): W1 data at scale (6wk)
-> W2 objects + diegetic UI (8wk) -> W3 simulation (4wk) -> W4 volumetrics (6wk)
-> W5 procedural synthesis (4wk) -> W6 UX (5wk) -> W7 perf/hardening (4wk)
-> W8 DR4 migration + data lifecycle (3wk).  ~40 weeks post-prototype.

THE BUILD LOG - MANDATORY, PART OF THE DELIVERABLE OF EVERY SESSION (PLAN 16.3):
  BUILDLOG.md                 append-only narrative. NEVER edit or delete an entry.
  docs/build/STATUS.yaml      machine-readable current state. REWRITTEN every session.
  docs/build/measurements.csv one row per (session x benchmark scene), append-only.
  docs/build/shots/           one dated screenshot per iteration.
  docs/adr/NNNN-*.md          one page per deviation: context/decision/consequences.
Every BUILDLOG entry MUST contain: goal, started-from commit, done (with file paths),
MEASURED numbers in a before/after/budget table (never adjectives - "faster" is
worthless, "11.2ms -> 4.1ms against a 5.5ms budget" is a record), blockers hit and how
they were resolved, work discovered but not done, a screenshot path, and a PRECISE
`Next:` specific enough that a cold reader can start without re-deriving context.
Record failures and dead ends - they are the most valuable content in the log.
RESUME PROTOCOL (in CONTRIBUTING.md, execute in order, skip nothing):
  STATUS.yaml -> last 3 BUILDLOG sessions -> relevant ADRs -> git log/status ->
  cargo build && test && run --scene orion_wide and compare to the last screenshot ->
  run --bench and compare to measurements_latest (>10% drift = investigate FIRST) ->
  only then start on next_action.  TRUST NOTHING THAT IS NOT VERIFIED BY RUNNING IT.
ALWAYS-WORKING RULES: main is always runnable, no long-lived branches, CI on every
commit (build + clippy -D warnings + test + precision torture + determinism + golden
images). A red main stops all other work. No feature is "done" until it has a measured
number attached. Every session ends: test green -> screenshot -> measurements row ->
STATUS.yaml -> BUILDLOG entry -> commit, in that order.

MILESTONES AFTER PHASE 0: M1 core app (6wk) -> M2 data at scale (6wk) -> M3 objects (8wk)
-> M4 simulation (4wk) -> M5 volumetrics (6wk) -> M6 UX (5wk) -> M7 perf/hardening (4wk)
-> M8 DR4 + data lifecycle (3wk). ~48 weeks total.

VALIDATION (see §17): differential astrometry vs JPL Horizons (<1 arcmin planets,
<1 arcsec stars), astropy round-trip transforms (<1e-12 rad), Harre-Heller colour
check (dE < 3), synthesis calibration vs real Gaia counts in (l,b,m) cells (<10%),
20 named perf benchmark scenes with >10%-regression CI failure, golden images with
perceptual diff, fixed-seed bit-identical determinism test, 24h warp soak.

NON-NEGOTIABLES: no f64 in WGSL (CI lint); never block the render thread on I/O;
never invent data - NaN/unknown is valid and must render as "unknown"; every LOD
transition is a dithered cross-fade; provenance is surfaced in the UI; the perf/debug
overlay exists from week 1; check §2.2 before adding scope.
