# PROJECT MILKY WAY — Master Engineering Plan

**Working title:** `galaxia` (rename freely; all paths below are relative to repo root)
**Platform:** Windows 10/11 x86-64 (Vulkan via wgpu). Linux/macOS must remain buildable but are not validated targets.
**Language:** Rust (edition 2024), WGSL shaders. Python 3.12 used *only* for the offline data ETL.
**Document status:** v1.2 — authoritative build instruction.
**v1.1 (2026-10-23):** target GPU confirmed as NVIDIA TITAN RTX; §1.1 rewritten; §6.2/§6.3 budgets revised for Turing; R1/R5 updated.
**v1.2 (2026-10-23):** disk confirmed at **200 GB free NVMe** → new §8.4.1 (streaming-partitioned ETL, mandatory) and §8.5 tiers revised. Prototype region confirmed as **Orion** (§16.2). Directive "prioritise a working prototype and iterate on it" → **§16 fully restructured** around a walking skeleton, a 7-rung iteration ladder, and post-prototype iteration waves. **New §16.3: the mandatory build log and resume protocol.** §4.1, §19, Appendix A updated.
**Audience:** An expert Rust + real-time-graphics engineer (human or AI) executing this build in a fresh session.

---

## 0. HOW TO USE THIS DOCUMENT

This plan is deliberately prescriptive. It contains:

- **§1 Decision Log** — the locked choices and their downstream consequences. Read first. Do not relitigate.
- **§2 Non-goals** — what we are explicitly *not* building. Check this before adding scope.
- **§3 Hard problems** — the three engineering risks that can kill the project, with the chosen mitigation for each.
- **§4–§12 Subsystem specifications** — architecture, data structures, algorithms, budgets.
- **§13 Data sources** — the answer to "what data sources are best", as a tiered table with licenses and access methods.
- **§14–§18 UX, milestones, risks, validation, repo layout.**
- **Appendix A** — a condensed build brief suitable for pasting into a fresh context window.
- **Appendix B–E** — crate list, data-source catalogue, glossary, references.

**Convention:** `[MUST]` is a hard requirement. `[SHOULD]` is strongly preferred, deviate with an ADR. `[MAY]` is optional. `[RISK]` flags a known hazard. `[VERIFY]` flags a claim that must be checked against reality during the build rather than trusted.

**Working rule for the build session:** when this plan and measured reality disagree, reality wins — but write an ADR (`docs/adr/NNNN-*.md`) recording the deviation and why.

### Contents

| § | Section | Read when |
|---|---|---|
| 0 | How to use this document | now |
| 1 | **Decision log** + 1.1 **target hardware (TITAN RTX)** | now — locked, do not relitigate |
| 2 | Scope + **non-goals** | before adding anything |
| 3 | **The three hard problems** (precision / density / volumetrics) | now — before writing code |
| 4 | System architecture (workspace, tick order, frame graph) | Iteration 0 |
| 5 | Coordinates, units, precision | Iteration 3 |
| 6 | Rendering architecture (passes, budgets, culling, governor) | Iteration 0–2 |
| 7 | Star & object rendering detail (photometry, rasteriser, glare, dust, compact objects) | Iteration 2, W2, W4 |
| 8 | Data layer — incl. **8.4.1 the 200 GB constraint**, 8.5 tiers, 8.7 synthesis | Iteration 1 |
| 9 | Simulation & time (ephemeris, light-time, transients) | Iteration 5, W3 |
| 10 | Camera & navigation, **10.3 the diegetic label system** | Iteration 0, W2 |
| 11 | Object model, taxonomy, legend, colouring modes | W2 |
| 12 | Search, curated lists, selection | Iteration 5, W6 |
| 13 | **Data sources — the tiered answer** + licensing + the DR4 plan | Iteration 1, W1 |
| 14 | Display / render / simulation settings | W6 |
| 15 | Visual style guide | Iteration 2 onward |
| 16 | **BUILD PLAN — prototype first, iterate; 16.3 the build log** | now — this governs *how* you work |
| 17 | Testing & validation | Iteration 6, W7 |
| 18 | Risk register | at each wave boundary |
| 19 | Build-session operating instructions | now |
| A | **Condensed build brief** (paste into a fresh session) | when starting a session |
| B | Crate and dependency list | Iteration 0 |
| C | Data source catalogue (detail, URLs, sizes, licences) | Iteration 1, W1 |
| D | Glossary | as needed |
| E | References & reference implementations | Iteration 2, 4 |

---

## 1. DECISION LOG (LOCKED)

These were decided by the product owner on 2026-10-23. Each row lists the decision and the consequences it forces on the rest of the design.

| # | Decision | Consequences |
|---|---|---|
| D1 | **Personal / learning project. Never distributed.** | Gaia's CC BY-NC 3.0 IGO license is fine. No installer, no code signing, no store, no DRM, no marketing. No telemetry. Ship criteria = "it runs on my machine and delights me". Massive scope freedom, zero compatibility burden. Still keep a `LICENSES/` dir with attributions — it costs nothing and science data deserves credit. |
| D2 | **Hybrid star population.** Real Gaia where we have it; statistically-faithful procedural synthesis beyond that, driven by a Besançon/Robin-class Galactic model. | Requires (a) a provenance bit on every object, (b) a validated synthesis model, (c) two code paths for star data that converge on one GPU struct. The synthesis model must be *calibrated against real Gaia counts* in the overlap region or the galaxy will look wrong at the seam. |
| D3 | **Zoom depth: down to planetary orbit / close flyby. No surface landing.** | Planets = shaded spheres with real albedo maps, phase, rings, atmosphere shells. No clipmap terrain, no DEM streaming, no surface lighting, no scatterable regolith. This removes roughly half the total project size versus SpaceEngine parity. |
| D4 | **Solar System fidelity: Keplerian elements + major perturbations.** Not full SPICE. | No NAIF SPICE dependency, no kernel management, no export-control review. Accuracy target: arcminutes for major planets, ~10⁻⁴ rad for moons. Ephemeris is computed, not interpolated from kernels. `[MAY]` add a DE440 SPK reader later as an accuracy upgrade behind the same trait. |
| D5 | **Stack: wgpu + custom renderer + custom frame graph.** No Bevy. | We own the render loop, the ECS-analogue, the asset/streaming layer, the windowing glue. More work, total control over indirect draws, compute culling, and buffer layouts — which is exactly what a billion-star GPU-driven pipeline needs. egui is used only for the settings/debug overlay (D11). |
| D6 | **Prototype first, then iterate on it.** (Refined 2026-10-23: a *walking skeleton* by day 2, then a 7-rung iteration ladder to a working prototype in ~5–6.5 weeks.) | §16 is structured as iterations, not spikes. Every iteration ends runnable, measured, screenshotted, and logged. Depth before breadth; the one exception is the precision architecture, which must land by Iteration 3. The project can be honestly abandoned or reshaped at the §16.7 exit review. |
| D7 | **Build on Gaia DR3 now; migrate to DR4 in January 2027.** | The ETL *must* be versioned and adapter-based from day one (§8.2), otherwise December 2026 forces a rewrite. DR4 lands **2 December 2026** with ~2.8 B sources and a new data model. Budget a dedicated 2-week migration milestone (M8) rather than treating it as a patch. |
| D8 | **Tiered on-disk install: Core ~20 GB / Standard ~80 GB / Full ~300 GB.** | The runtime must treat the dataset as *optional per region*: absent data → procedural synthesis or "not installed" markers, never a crash. Needs a manifest + capability query system, and a downloader with resume and checksums. |
| D9 | **Art direction: science-faithful. Beauty emerges from physics.** | Physically-based photometry, real dust extinction, HDR + exposure adaptation. No invented nebulae, no fake colours, no artistic density boosts in the default mode. Volumetrics are *seeded and shaped by real catalogued data*. Where the sky is genuinely empty, it renders empty. A clearly-labelled non-physical "enhanced visibility" gain mode is permitted as an *option* (§7.7) because otherwise faint real structure is invisible on an SDR display. |
| D10 | **UI: diegetic 3D.** Floating labels anchored in world space, radial menus, in-world info panels. | This is the single most under-appreciated cost in the decision set. Labels across 25 orders of magnitude of zoom require a budgeted, decluttered, scale-aware label system (§10.2) and a GPU SDF text renderer. Not a weekend of egui. |
| D11 | **No scripting, no cinematics, no remote-control API.** Pure GUI app. | Removes Rhai/Lua embedding, keyframe systems, recording, and the plugin surface. The settings overlay *may* use egui since it is not part of the diegetic fiction — treat it as a "cockpit maintenance panel". |
| D12 | **Build session assumed expert in Rust and graphics.** | This document is terse and high-density. Rendering concepts are named, not explained. Rationale is given only at decision points. |

### 1.1 Target hardware — CONFIRMED 2026-10-23

> **Reference target: NVIDIA TITAN RTX** — Turing TU102, 4 608 CUDA cores (36 SMs), 576 Tensor Cores, 72 RT Cores, 288 TMUs, 96 ROPs, **24 GB GDDR6 on a 384-bit bus at 672 GB/s**, 1770 MHz boost, ~16.3 TFLOPS FP32. PCIe 3.0 x16 (~15.75 GB/s). DisplayPort 1.4, 8K@60 capable. NVLink-capable (2 cards → 48 GB at 100 GB/s).
>
> **Host:** 96 GB RAM, 32 hardware threads.

#### 1.1.1 What this hardware means for the plan — read carefully

**VRAM: no longer the constraint.** 24 GB is ample; the revised §6.3 budget uses 7.3 GB and leaves ~14.7 GB of headroom. Nothing in the plan changes on the memory side — the star hot set alone is only ~0.7 GB. If a second card is ever added via NVLink, 48 GB makes the Full tier fully resident — but `[MUST NOT]` design for multi-GPU in v1 (split-frame rendering is a large project for a personal one, and it would not solve the compute bottleneck anyway).

**Compute throughput: now the binding constraint.** The Titan RTX is a 2018 Turing card. Relative to the RTX 4070 Super used as the reference in the Eurographics 2025 star-rasterisation paper, it has roughly **2.2× fewer shader ALUs** (16.3 vs ~35.5 TFLOPS FP32) — memory bandwidth is comparable (672 GB/s vs 504 GB/s, actually in the Titan's favour). The decisive disadvantage is **L2 cache: Turing's 6 MB versus Ada's 48 MB**, which matters enormously for an atomic-heavy software rasteriser, plus a two-generation gap in compute IPC.

`[ESTIMATE — to be falsified by P0-S2]` Realistic extrapolation:

| Workload | 4070 Super (measured, published) | Titan RTX (extrapolated) |
|---|---|---|
| Software-rasterise 22 M in-frustum stars @ 4K | 8.0 ms | **~16–22 ms** |
| Screen-space glare pyramid @ 4K | 0.74 ms | ~1.5 ms |
| View-space asymmetric glare @ 4K | 1.38 ms | ~2.8 ms |

**The consequence, stated plainly:**

> **Native 4K60 with 20 M visible stars plus volumetrics plus post-processing is NOT achievable on a Titan RTX.** The star rasteriser alone would consume the entire 13 ms budget.

**The achievable target — and this is now the plan of record:**

1. **Internal render resolution 2560×1440, upscaled to 3840×2160.** 1440p is 2.25× fewer pixels than 4K. Because the star rasteriser's cost is dominated by *fragments written*, not stars traversed, this cuts its cost by ~2–2.5×, bringing 20 M stars to ~7–9 ms.
2. **Upscaler: FSR2 (primary), XeSS DP4a (secondary).** DLSS is unavailable through wgpu. XeSS *does* run on Turing via DP4a, but Digital Foundry's testing shows the DP4a path is visibly weaker than both XMX XeSS and DLSS. **Use FSR2 as the default** — it is vendor-neutral compute and behaves consistently on Turing.
3. **Star budget: 8–10 M visible at 1440p internal for a comfortable 60 fps**, 15–20 M when volumetrics are off or degraded, and the governor (§6.6) trades between them continuously.
4. **Native 4K remains available as a user setting** and will hold 60 fps in *local* views (planetary scale, where few stars are visible) — just not in dense galactic-plane views. That is an honest, defensible product position.

`[MUST]` P0-S2 measures the real numbers on the real card in week 3. If the extrapolation above is wrong in either direction, the star budget and default internal resolution are updated then — **not** in month 8.

#### 1.1.2 Turing feature floor (hard constraints on the renderer)

| Capability | Status | Consequence |
|---|---|---|
| Vulkan 1.3, SM 7.5 | ✔ | — |
| `VK_EXT_descriptor_indexing` (bindless) | ✔ | Bindless texture arrays for imagery tiles are available. |
| `VK_EXT_shader_atomic_float` (atomic float add) | `[VERIFY]` on the actual driver | If absent, use the per-tile u32 fixed-point accumulation + frame-global luminance normalisation from the 2025 paper. **Build the fallback path in P0-S2, do not discover the need for it in M3.** |
| DP4a / Int8 dot product | ✔ | Enables XeSS DP4a; also useful for quantised attribute unpacking. |
| Int64 atomics | ✘ | No 64-bit counters. Use per-tile u32 with a wrap-around-safe design. |
| Mesh shaders | ✘ (not exposed on NVIDIA Vulkan) | No meshlet pipelines. Irrelevant — stars are rasterised in compute. |
| Hardware ray tracing in wgpu | Incomplete upstream | Not used (§2.2). Turing *has* 72 RT cores but do not plan around them. |
| FP8 / SER / meshlets / DLSS3 frame gen | ✘ | Ada/Hopper-only. Not used. |
| PCIe 3.0 x16 (~15.75 GB/s) | ⚠ | **Host→device uploads are capped at ~15 GB/s.** Streaming 600 MB/s during warp travel is fine (4 % of the bus), but `[MUST]` never design a per-frame path that uploads tens of megabytes. All bulk data goes up once and is transformed on-device. |
| 6 MB L2 (vs Ada's 48 MB) | ⚠ | The dominant reason the star rasteriser is proportionally slower here than raw TFLOPS suggests. Favour tile-local/shared-memory accumulation over global atomics; keep working sets L2-friendly. |

#### 1.1.3 Host machine assessment (96 GB RAM / 32 threads)

**Comfortably sufficient, and better than the plan assumed.**

- **Gaia DR3 full ingest** (~1 TB compressed CSV / ~2 TB unpacked): DuckDB/Polars streaming reads across 32 threads. Expect **6–14 h per full pass**, and the pipeline needs ~3 passes (ingest → astrom/classify → octree+quantize). Budget **a full weekend** for a complete DR3 ingest, and note that 96 GB RAM means large hash joins during cross-matching (§8.2 stage 5) can run in memory rather than spilling.
- **Octree build** for 3 × 10⁸ stars (Morton sort + leaf packing): parallelisable across all 32 threads, expect **1–3 h**.
- **Runtime:** the render loop is mostly single-threaded plus 2–4 streaming threads. 32 threads is far more than needed; the app will not be CPU-bound. `[MAY]` use the spare cores for asynchronous ETL re-bakes while the app runs.
- **`[CONFIRMED]` 200 GB free NVMe.** This is a **hard constraint that reshapes the entire data strategy** — see §8.4.1. The full Gaia CSV dump (~1 TB compressed, ~2 TB unpacked) **cannot be downloaded**, and neither can a single-pass all-sky raw extract at G < 20 (~400–600 GB). The ETL must therefore be **streaming-partitioned**: query → process → write only the compact quantised output → delete raw → next partition. Peak disk usage must stay under ~60 GB at all times.

#### 1.1.4 Frame budget restated for Titan RTX

16.67 ms/frame → reserve 3.0 ms CPU → **GPU budget 13.0 ms at 2560×1440 internal**, then FSR2 → 4K. See §6.2 for the revised per-pass table.

---

## 2. SCOPE

### 2.1 In scope

1. Navigable, continuous-zoom 3D Milky Way from 100 kpc exterior views down to ~10⁴ km from a planet.
2. Real astrometry and photometry for hundreds of millions of stars; procedural synthesis beyond the catalogued volume, clearly flagged.
3. ~10⁵–10⁶ catalogued deep-sky objects (nebulae, clusters, remnants, galaxies), each classified, selectable, and individually rendered with an archetype appropriate to its class.
4. Solar System with Keplerian+perturbed ephemeris for planets, major moons, ~10⁴ small bodies, and comet activity.
5. ~6 000 confirmed exoplanet systems from the NASA Exoplanet Archive, rendered as real orbital configurations.
6. Volumetric ISM: dust lanes, molecular clouds, H II regions, planetary nebulae, SNR shells, superbubbles — driven by 3D dust maps and catalogued geometry, with volumetric lighting and crepuscular rays.
7. Relativistic-jet and accretion phenomena for AGN, microquasars, and YSOs, with synchrotron-style emission and Doppler asymmetry.
8. Full HDR pipeline: physical star colours, glare/bloom via a perceptual pyramid, auto-exposure with adaptation, filmic tone mapping, optional FSR2/XeSS.
9. Diegetic UI: world-anchored SDF labels with decluttering, radial context menus, in-world info panels, an interactive classification legend with filtering, a time dial, and a physical scale bar.
10. Object search: names, aliases, catalog designations, coordinates, and faceted queries; plus ~35 curated "extraordinary objects" lists generated from real catalog filters.
11. Time simulation from −10⁶ yr to +10⁹ yr with proper motion, light-time correction, precession/nutation, and discrete jumps to notable epochs.
12. GPU-driven dynamic LOD with a closed feedback loop on measured frame time, hierarchical frustum + occlusion culling, and depth culling inside dense clouds.
13. Display, render, and simulation settings, all persisted.

### 2.2 Non-goals (do not build these; do not "just quickly add" these)

| Non-goal | Why | Revisit when |
|---|---|---|
| Planetary surface landing, terrain, DEM streaming | D3. Doubles the renderer's scope. | Never, in v1. |
| Full N-body integration of the Solar System | D4. Visual payoff ≈ 0, cost high, and it makes ephemeris *less* accurate than an analytical theory. | Only as a sandbox toy mode post-1.0. |
| JPL SPICE kernels | D4. Export-control review, C FFI, kernel management. | If arcminute accuracy proves insufficient — see §9.2 escape hatch. |
| Ray-traced reflections/refractions/shadows | wgpu RT is incomplete; a space sim has almost nothing to ray trace against. | Never for v1. |
| Multiplayer, networking, accounts, cloud saves | D1. | Never. |
| Scripting (Rhai/Lua), plugin API, remote control | D11. | Never. |
| Cinematic camera paths, keyframing, video recording | D11. Use OS-level capture (e.g. Xbox Game Bar / OBS). | Never. |
| VR / stereo / dome output | Not requested; would constrain the whole renderer. | Post-1.0, and note the glare work in §7.4 is already perspective-correct-capable. |
| Android / macOS / WebGL / WASM | Windows-only per brief. Keep the code *portable* (no Win32-specific rendering paths) but do not test or optimise. | Never for v1. |
| Full extragalactic survey rendering (millions of galaxies) | Only ~5 000 curated galaxies + Local Group + clusters as sprites/impostors. | Post-1.0. |
| Asteroids beyond ~10⁴ | Rendering 1.4 M MPC orbits is a data problem with negligible visual return. | Post-1.0, behind a setting. |
| Community-mod support, custom catalog import UI | D1. The ETL is the import mechanism. | Never. |
| Localisation | English only. All UI strings via a single `strings.rs` so it is not architecturally blocked. | Never for v1. |

---

## 3. THE THREE HARD PROBLEMS

Everything else in this project is well-trodden. These three are not, and they determine success.

### H1 — Scale and floating-point precision (25 orders of magnitude)

**The problem.** The scene spans ~200 kpc (≈ 6.2 × 10¹⁷ km ≈ 4.1 × 10¹² AU) down to a camera 10⁴ km from a planet — a ratio of ~4 × 10²⁰, about **20.6 orders of magnitude** (≈ 68 bits of *range*). f32 has 24 bits of mantissa: at 1 AU, an f32 metre-resolution value is already unusable; at 1 kpc, f32 quantisation is ~0.06 AU ≈ 9 × 10⁶ km. Rendering a planet's disc at that distance with f32 world coordinates produces visible jitter, z-fighting, and vertices snapping to a lattice. f64 has 53 bits (~15.9 decimal digits) — enough for range, but GPUs are 1/32-rate or worse on f64, and WGSL **has no f64 type at all**.

**The solution — three layers, all mandatory:**

**Layer A: Canonical storage in f64, hierarchical reference frames.**
Every body belongs to a `Frame` in a tree rooted at the Solar System Barycentre (SSB), with secondary roots at the Galactic Centre (for galactic-scale objects) and the Local Group barycentre (for extragalactic). A body's position is stored as `f64` **relative to its parent frame's origin**, not absolute. This keeps mantissa bits where they are needed: a moon stored relative to its planet has full f64 precision at kilometre scale, unaffected by the planet's 30 AU offset from the Sun.

**Layer B: Per-frame rebasing (floating origin).**
Each frame, the engine selects a `render_frame` — the deepest frame that contains the camera within a comfortable margin. All render-space positions are computed on the CPU in f64 as `(object_absolute − camera_absolute)`, then downcast to f32. When the camera crosses a threshold (default: 25 % of the current frame's characteristic radius), the render frame is swapped and **all transforms are recomputed in the same frame** — the swap is invisible because it happens in f64 before the downcast. Threshold crossing must be edge-triggered with hysteresis to prevent oscillation at boundaries.

**Layer C: Split-double (double-single) view transform in the shader.**
Even after rebasing, an object 10 kpc away rendered in f32 relative to the camera loses precision. Fix: pass the view translation to the GPU as two f32s (`hi`, `lo`) such that `hi + lo` reconstructs the f64 to ~48 bits, and compute the relative offset in the vertex shader as `((obj_hi - cam_hi) + (obj_lo - cam_lo))` using the Knudsen/Stulpner two-sum sequence. This is the standard technique from planetary renderers (Cesium, Outerra, CosmoScout) and is the difference between "smooth" and "shimmering".

**Additional mandatory measures:**
- **Reversed-Z** infinite-far projection (`near = 1.0`, `far → ∞` via the `[1,0]`-row trick) with a **per-frame near plane** derived from the closest visible geometry. Depth precision then spans the whole range without a logarithmic hack.
- **No f64 in WGSL.** All double math is CPU-side or split-double. `[MUST]` Add a CI lint that fails the build on `f64` appearing in any `.wgsl` file.
- **A precision torture test** as the very first spike (§16 P0-S1): render Sun and Proxima Centauri in the same frame, then zoom continuously from 100 kpc to 10⁴ km with a fixed star and assert sub-0.1-pixel centroid stability. This test either passes or the whole architecture is wrong; there is no partial credit.

`[RISK]` The most common failure mode is *silent*: the app looks fine at galactic zoom and fine at planetary zoom, but stutters during the transition. Instrument the render-frame swap with an on-screen counter during development.

### H2 — Density: 10⁸–10⁹ stars against 24 GB VRAM within a 13 ms frame

**The problem.** Even the "core" tier holds ~3 × 10⁸ stars. At 32 bytes each that is 9.6 GB of *raw* attribute data — and it is the wrong data: 99.99 % of those stars are never visible at once, and the visible ones change every frame as the camera moves.

**The solution — five compounding mechanisms:**

1. **Spatial LOD octree, resident on the CPU, streamed to the GPU.** An octree over galactic coordinates, Morton-ordered, with ~10⁵ stars per leaf → for 3 × 10⁸ stars that is ~3 000 leaves / ~6 000 nodes; for the full 1.5 × 10⁹ catalog, ~30 000 leaves / ~60 000 nodes. The **node array (~7 MB) is permanently resident**; only leaf particle payloads stream. Each node stores a cumulative brightest-magnitude and a bounding sphere, so a whole subtree can be rejected with one test.

2. **A physically-motivated LOD criterion, not a distance heuristic.** A star is worth rasterising if its projected luminance exceeds a threshold. For a node at distance `D`, render its contents iff
   `M_brightest_node < m_threshold(D) = m₀ − 5·log₁₀(D / D₀)`
   This is exactly Gaia Sky's minimum-solid-angle θ criterion restated in magnitudes, and it produces the correct behaviour for free: the dense Galactic centre resolves deep, while the halo thins out naturally. `m₀` is the *dynamic* parameter the feedback loop (§6.6) drives.

3. **GPU-driven selection into an indirect draw buffer.** Compute shader walks the visible node list, applies the criterion per star, and appends survivors via `atomicAdd` into a compacted `DrawIndexedIndirect` / `DrawIndirect` buffer plus a per-star attribute stream. The CPU never sees the selection. Expected visible-star counts: 2–8 M at planetary scale, 15–40 M at 200 pc, 5–20 M at galactic scale (halo is sparse).

4. **Software rasterisation of sub-pixel stars.** This is the key performance lever and it is *proven*: Schneegans, Kreskowski & Gerndt (Eurographics 2025), "Smaller than Pixels: Rendering Millions of Stars in Real-Time", rendered **22 M in-frustum Gaia stars in ~8 ms at 4K on an RTX 4070 Super** using a compute-shader software rasterizer with atomic luminance accumulation, and showed it beats both `GL_POINTS`-style point rendering and billboard quads in nearly all regimes. On a **Titan RTX** the extrapolated cost is ~2–2.5× that (see §1.1.1), so the operating point becomes **8–10 M stars at 2560×1440 internal within 5.5 ms**. This still implies the rasteriser is the right choice and billboards are wrong — billboards would cost more, not less, because of overdraw.
   - Rasterise into an **R11G11B10Float or RGBA16Float HDR target** (stars must be additive and HDR; an 8-bit target destroys the dynamic range that makes bright stars look bright).
   - Use a **spin-lock-free approach where possible**: prefer per-tile atomic float adds on a 32×32-tile scratch buffer, or the Schütz/Kerbl/Wimmer per-pixel linked-list-free approach. `[VERIFY]` atomic float `add` on RGBA16F in Vulkan requires `VK_EXT_shader_atomic_float` — **check exposure on the actual Turing driver in P0-S2**; if unavailable, fall back to per-tile fixed-point u32 accumulation with a shared luminance scale (the paper does exactly this normalisation trick). Given Turing's 6 MB L2, **favour shared-memory tile accumulation resolved once per tile over per-fragment global atomics** — this is likely the single biggest Titan-RTX-specific optimisation.

5. **VRAM budget with an LRU resident-set manager.** Hot set: 10 M stars × 32 B = 320 MB of particle data, plus a 300 MB streaming double-buffer, plus ~7 MB octree. Total star budget **~0.7 GB**; even a 20 M hot set is only ~1 GB. Against the Titan RTX's 24 GB this is not a constraint at all — the §6.3 budget uses 8.2 GB and leaves ~14 GB for streaming spikes during warp travel.

**The streaming hard case: warp travel.** Jumping to M31 (780 kpc) invalidates the entire resident set. Strategy:
- On a warp command, immediately re-root the octree traversal at coarse LOD (levels 0–4, always resident) so *something* correct appears within one frame.
- Refine asynchronously with priority = f(solid angle, brightness, distance from the travel target), over ~1–3 s.
- Show a diegetic "transit" state rather than a spinner: the view holds at coarse LOD with correct photometry, which is honest and looks intentional.
- Bandwidth: at coarse LOD the whole galaxy is ~50 MB, comfortably inside the always-resident set. Deep refinement streams at up to ~600 MB/s worst case — fine on NVMe Gen4, marginal on SATA. `[MUST]` gate the Full tier behind an NVMe check and warn otherwise.

### H3 — Volumetrics and HDR in a scene with no ground and no bounded light

**The problem.** "Volumetric clouds, volumetric lighting, glowing objects, light rays, plasma clouds and jets" in a space sim is much harder than in a terrain game: there is no bounded slab to march, the density field spans parsecs to kilometres, light sources are point-like at enormous dynamic range (a B0 star is 10⁵ × solar luminosity), and the same raymarcher must work when the camera is *inside* a molecular cloud and when it is 5 kpc away looking at it.

**The solution — a froxel volume with a hybrid density field, and scattering that is cheap but not fake:**

- **Froxel grid** (frustum-aligned voxels) at **quarter resolution**, `160 × 90 × 64` with logarithmic depth slicing. That is 921 600 froxels — injectable and scatterable in ~0.6 ms, raymarchable in ~2.0 ms. Temporal reprojection at **1/8 rate** with blue-noise jitter and neighbourhood clipping (the Schieder/Eitzinger *Optimisations for Real-Time Volumetric Cloudscapes* recipe, and the Horizon Zero Dawn 1/16-rate trick) brings the amortised cost to ~0.5 ms with a quality cost that is invisible when the camera moves slowly and acceptable when it does not.
- **Hybrid density field.** Three additive contributions:
  1. **Global 3D dust maps** sampled as a coarse 3D texture / point-cloud texture — Edenhofer et al. (2024) parsec-scale out to 1.25 kpc (Zenodo 10658339), and Dharmawardena et al. (2024) all-sky out to 2.8 kpc. Beyond that, a Planck/Haslam-derived column-density shell plus a Galactic-model falloff. This is what makes dust lanes appear in the *right places* — the single biggest contributor to looking real.
  2. **Per-object analytic volumes** for the ~10⁴ catalogued nebulae, each parameterised from real data (angular radius, distance, Hα/O III/S II fluxes where available, excitation class, morphology class). Shapes from low-frequency noise warped by the object's class template (bipolar PN, shell SNR, cometary globule, superbubble).
  3. **Procedural detail octaves** (Worley-FBM + curl-noise advection) that add structure *within* an object's envelope but never create an object.
  This satisfies D9: every cloud corresponds to a real catalogued thing or a real dust map; the procedural layer only redistributes density inside it.
- **Volumetric lighting.** Single scattering with a **6-step cone march toward the dominant light source**, plus 2-octave spherical-harmonic ambient from a coarse precomputed irradiance probe (recomputed on focus change, not per frame). Beer–Powder for the shadow term so dense cores go properly dark. **Crepuscular rays fall out for free** — no separate god-ray pass — because the shadow march toward a bright star inside a dusty volume *is* the god ray. This is both cheaper and more correct than a screen-space radial blur.
- **Emission.** Three physically distinct paths, all data-seeded:
  - *Ionised gas* (H II regions, PNe, SNR shells): line emission. Use the real three-line palette — Hα 656.3 nm, [O III] 500.7 nm, [S II] 671.6/673.1 nm — with ratios set by the object's catalogued excitation. Render in linear spectral space and let the tone mapper produce the colour. Result: the Pillars of Creation look like the Pillars of Creation *because the physics is right*, not because someone painted them.
  - *Synchrotron* (AGN jets, pulsar wind nebulae, SNR shells): power-law spectrum, strongly polarised, **Doppler-beamed** — the approaching jet is brighter and bluer than the receding one. This asymmetry is real, is visible in real images (M87), and is nearly free to implement. It is also the most convincing "this simulation knows physics" signal in the whole app.
  - *Thermal dust* (dark clouds, cirrus): re-radiation in the IR, so in the visual band these are *extinction*, not emission. Getting this backwards is the classic amateur mistake — a dark nebula must occlude, not glow.
- **Empty-space skipping and early exit.** Build a coarse mip of the density field; skip spans where max density ≈ 0; terminate when transmittance < 1 %. Inside a dense cloud this is what keeps the march bounded.
- **Depth culling inside dense clouds** (explicitly requested): three cooperating mechanisms —
  1. **Two-pass occlusion**: a half-res depth prepass of opaque geometry feeds the star rasteriser, which discards occluded fragments. Essential when the camera is behind a planet inside a cloud.
  2. **Per-froxel occupancy saturation**: track particles-contributing-per-froxel; once a froxel's optical depth exceeds τ_max, stop injecting there. Prevents the "inside the Orion Nebula, everything is white" failure.
  3. **Adaptive density gain**: the feedback loop (§6.6) measures mean optical depth along primary rays and reduces injection density when the frame is over-budget. This is the "render count" knob the brief asks for, and it is closed-loop rather than a slider the user has to fiddle with (though a slider sets its ceiling).

`[RISK]` Volumetrics is where frame time goes to die. **Hard rule: volumetrics is the first thing degraded under frame pressure** (resolution 1/4 → 1/8 → 1/16, steps 96 → 48 → 24, temporal 1/8 → 1/16 → 1/32) and the last thing enabled. Never let it be the reason stars stutter.

---

## 4. SYSTEM ARCHITECTURE

### 4.1 Workspace layout

Cargo workspace, one crate per subsystem, hard dependency direction (no cycles):

```
galaxia/
├── Cargo.toml                    # workspace
├── rust-toolchain.toml           # pin stable; nightly only for specific lints
├── .cargo/config.toml            # target-cpu=native for release, incremental settings
├── crates/
│   ├── galaxia-core/             # units, math, precision, ids, error, config, time
│   ├── galaxia-astro/            # coordinates, photometry, stellar physics, dust, taxonomy
│   ├── galaxia-ephemeris/        # Kepler solver, planetary theory, small bodies
│   ├── galaxia-data/             # schemas, octree, streaming, resident set, manifests
│   ├── galaxia-render/           # wgpu device, frame graph, all passes, WGSL
│   ├── galaxia-scene/            # frame hierarchy, scene graph, selection, queries
│   ├── galaxia-sim/              # time, proper motion, light-time, transients, synthesis
│   ├── galaxia-ui/               # diegetic labels, radial menus, info panels, legend, SDF text
│   ├── galaxia-search/           # index build/query, faceted queries, curated lists
│   ├── galaxia-app/              # the binary: event loop, state machine, wiring
│   └── galaxia-bench/            # criterion benches + standalone GPU benchmarks
├── etl/                          # Python 3.12 — offline data pipeline (NOT in the workspace)
│   ├── pyproject.toml            # astroquery, astropy, duckdb, polars, healpy, tqdm
│   ├── sources/                  # one module per catalog: gaia.py, simbad.py, nasa_neoo.py …
│   ├── normalize.py              # → common schema
│   ├── classify.py               # rule-based taxonomy assignment
│   ├── crossmatch.py             # identity resolution
│   ├── octree_build.py           # Morton sort + LOD pyramid
│   ├── quantize.py               # → binary columnar artifacts
│   ├── validate.py               # differential astrometry tests vs Horizons/Stellarium
│   └── lineage.py                # provenance/credits manifest generation
├── shaders/                      # .wgsl, included via naga or wgpu's shader module loader
├── assets/                       # noise LUTs, SDF font atlas, BRDF LUTs, filter curves, albedo maps
├── data/                         # gitignored; generated artifacts land here
├── BUILDLOG.md                   # ★ append-only build log (§16.3) — the resume record
├── CONTRIBUTING.md               # contains the resume protocol, verbatim (§16.3.3)
├── docs/
│   ├── build/
│   │   ├── STATUS.yaml           # ★ machine-readable current state — rewritten every session
│   │   ├── measurements.csv      # ★ every recorded number, append-only
│   │   └── shots/                # ★ one screenshot per iteration, dated
│   ├── adr/
│   │   ├── INDEX.md              # one line per ADR
│   │   └── NNNN-*.md
│   ├── data_dictionary.md
│   ├── taxonomy.md
│   ├── visual_style_guide.md
│   ├── perf_budgets.md
│   └── validation_protocol.md
├── tests/
│   ├── astrometric/              # numeric assertions vs authoritative sources
│   ├── golden/                   # reference PNGs + perceptual diff harness
│   └── scenes/                   # named benchmark scenes
└── LICENSES/                     # third-party + data attributions
```

**Why a workspace, not a crate:** compile times. `galaxia-render` will be the slowest crate by far (WGSL validation, naga). Isolating it means a change to `galaxia-search` does not recompile shaders.

**Dependency rule:** `core` ← `astro` ← `ephemeris` ← `data` ← `scene` ← `render`/`sim` ← `ui` ← `app`. `galaxia-render` must not depend on `galaxia-scene` — it consumes plain GPU-buffer-ready structs. This is what keeps the renderer testable in isolation.

### 4.2 Application architecture

No heavyweight ECS (D5: no Bevy). Instead, a **struct-of-arrays scene store** with typed handles:

- `SlotMap<BodyId, Body>` for the ~10⁶ first-class objects (planets, catalogued DSOs, named stars).
- Stars are **not** entities. They are rows in streaming buffers addressed by `(node_id, index)`. Making 3 × 10⁸ stars entities is the classic architecture mistake here — it forces per-object overhead on data that only ever exists in bulk.
- `galaxia-app` runs a **fixed-order tick**: `input → time_advance → sim_update → frame_hierarchy_update → visibility_request → stream_pump → gpu_submit → ui_update → present`.
- Streaming runs on a **dedicated thread pool** (2–4 threads) with a lock-free MPSC of completed chunks. Never block the render thread on I/O. `[MUST]`
- All GPU-facing structs are `#[repr(C)]`, `bytemuck::Pod`, and validated by a `static_assertions::const_assert_eq!(size_of::<T>(), N)` in a test. Silent layout drift between Rust and WGSL is the most common source of "everything is slightly wrong" bugs.

### 4.3 Frame graph

Implement a minimal explicit frame graph: passes declare resource reads/writes and the graph computes lifetimes, aliases transient memory, and inserts barriers. This is ~800 lines and pays for itself immediately because this renderer has 15+ passes with heavy resource reuse (the depth prepass feeds occlusion *and* picking *and* volumetric depth bounds).

Do not over-engineer: no async compute graph compilation, no render-pass caching beyond one frame. `[MAY]` use wgpu render bundles for the opaque pass if profiling shows CPU-side encoding cost > 0.5 ms.

---

## 5. COORDINATES, UNITS, PRECISION

### 5.1 Canonical units

| Quantity | Storage | Unit | Notes |
|---|---|---|---|
| Absolute position | `f64 × 3` | **AU**, ICRF, J2000.0 equinox | AU gives ~1 mm precision at 100 kpc in f64 — ample. Avoids pc/kpc unit-mismatch bugs. |
| Relative position (within frame) | `f64 × 3` | AU | Relative to parent frame origin. |
| Render position | `f32 × 3` (split-double for the camera) | **scene units**, where 1 su = 1 AU at the current render frame | Rescaled per render frame; see §5.3. |
| Stellar mass / luminosity | `f32` | M☉, L☉ | log-encoded in GPU buffers. |
| Distance (public API) | `f64` | **parsec** | Convert to ly only at the display boundary. |
| Time | `f64` | **days since J2000.0 TDB** | JD − 2451545.0. |
| Angle | `f64` | radians internally; degrees at the UI boundary only | |
| Magnitude | `f32` | mag (Vega system unless stated) | Gaia G is its own system — never mix G and V without a transform. |

`[MUST]` Define these as newtypes in `galaxia-core::units` (`Au`, `Pc`, `Km`, `JulianDay`, `Mag`, `Kelvin`, `Lsol`, `Msol`) with explicit conversions. Unit confusion across a 20-OoM scene is unrecoverable otherwise. `#[deny(clippy::float_cmp)]` is too strict here; use `approx` with per-scale tolerances.

### 5.2 Reference systems

Support and interconvert, with the active system a user setting:

- **ICRF / J2000.0 equatorial** (α, δ) — the canonical storage frame.
- **Galactic (l, b)** — for Galactic structure work; IAU 1958 definition, north pole at (192.85948°, 27.12825°) J2000, ascending node 32.93192°.
- **Supergalactic** — for extragalactic views.
- **Ecliptic (J2000 and of-date)** — for Solar System work.
- **Alt-az (of-date, observer-lat/long)** — `[MAY]`, only as an "observe from Earth" mode. Requires precession + nutation + refraction; defer to M4.
- **CMB dipole frame** — `[MAY]`, a delightful mode: the Sun moves at 369.82 km/s toward (l, b) = (264.021°, 48.253°); rendering the sky in that frame removes the CMB dipole and is a genuinely educational view.

Transformations live in one module with **round-trip property tests** (`assert!(abs(rt(x) − x) < 1e-12)` over 10⁶ random sky positions). Validate against `astropy.coordinates` offline and bake the residuals into `docs/validation_protocol.md`.

### 5.3 The render-frame rescale

The f32 render space cannot hold both 100 kpc and 10⁴ km. Solution: **the render frame carries a scale exponent**.

```
render_position_f32 = (absolute_f64 − camera_absolute_f64) × 10^(−scale_exp)
```

where `scale_exp` is an integer chosen per render frame so that the scene's characteristic extent maps into ~[10⁻³, 10³] f32 units. Changing `scale_exp` on a frame swap requires **no visual discontinuity** because the projection matrix absorbs it (multiply the projection's scale, or equivalently rescale near/far). All *sizes* (planet radii, label anchor offsets, pick radii) must be multiplied by the same factor. `[MUST]` centralise this in one struct (`RenderFrameTransform`) and forbid any code from doing its own scaling.

### 5.4 Depth

- Reversed-Z, `near = 1.0`, infinite far. Depth range `[1, 0]`.
- Per-frame dynamic near plane: `near = clamp(closest_visible_extent × 0.1, 1e-4 su, 1e6 su)`.
- `[MUST]` The near plane is *derived*, never a user setting — a user-set near plane is how precision bugs get introduced.
- Volumetrics reconstruct view-space depth from the reversed-Z buffer with the standard `linear = near / (depth + epsilon)` form.

---

## 6. RENDERING ARCHITECTURE

### 6.1 Frame graph — passes in order

| # | Pass | Type | Resolution | Notes |
|---|---|---|---|---|
| 1 | `update_uniforms` | CPU + upload | — | Frame constants, camera split-double, time, exposure state. |
| 2 | `cull_octree` | Compute | — | Hierarchical node cull → visible node list (persistent GPU buffer, no readback). |
| 3 | `select_stars` | Compute | — | Per-star LOD test → compacted attribute buffer + indirect args + frame stats. |
| 4 | `depth_prepass` | Raster (half-res) | 1920×1080 | Opaque geometry only. Feeds occlusion + volumetric bounds + picking. |
| 5 | `raster_stars` | Compute | full or internal | Software rasteriser → HDR star target. Depth-tested against pass 4. |
| 6 | `opaque_gbuffer` | Raster | internal | Planets, moons, rings (alpha-tested), meshes, spacecraft. Albedo/normal/material + motion vectors. |
| 7 | `lighting` | Compute or raster | internal | Clustered forward or deferred. Dominant-light identification for volumetrics. |
| 8 | `accretion_disks` | Raster/compute | internal | Raymarched relativistic disks for BHs/neutron stars. §7.6. |
| 9 | `volume_inject` | Compute | 160×90×64 | Density + emission + extinction into the froxel grid. |
| 10 | `volume_scatter` | Compute | 160×90×64 | Single scatter + shadow march + SH ambient, 1/8-rate temporal. |
| 11 | `volume_integrate` | Raster | internal | Front-to-back raymarch of the froxel grid, composited with the HDR target using pass-4 depth. |
| 12 | `transparent` | Raster | internal | Comae, ion tails, jet cones, glass UI anchors. Back-to-front by depth. |
| 13 | `picking_id` | Raster (1/4-res) | 960×540 | Object-ID buffer for hover/click. Reuses pass-6 geometry with an ID output; stars get a separate GPU-side nearest-in-tile query. |
| 14 | `glare_bloom` | Compute ×N | pyramid | Perceptual glare via mip pyramid with 1/2ⁱ weights + bicubic reconstruction. §7.4. |
| 15 | `exposure_adapt` | Compute | — | Luminance histogram → key-value auto-exposure with temporal adaptation. |
| 16 | `tonemap_upsample` | Compute/raster | internal → 4K | Filmic curve + FSR2/XeSS upsample, or direct tonemap at native. |
| 17 | `taa_resolve` | Compute | 4K | Optional; must run *after* upsample if both are used. |
| 18 | `labels_ui` | Raster | 4K | GPU SDF text, leader lines, selection rings, radial menus. Depth-aware. |
| 19 | `post_optional` | Raster | 4K | DoF, chromatic aberration, grain, vignette. Off by default (D9). |
| 20 | `readback_stats` | Async readback | — | Frame stats for the feedback loop. Deliberately 2 frames delayed. |

### 6.2 GPU frame budget — TITAN RTX @ 2560×1440 internal → 4K output

13.0 ms usable (16.67 ms frame − 3.0 ms CPU). Internal resolution 1440p; FSR2 upsamples to 3840×2160.

| Pass group | Budget | Notes |
|---|---|---|
| Cull + select (2–3) | 0.6 ms | ~60 k nodes, ~20 M stars tested. Compute-bound; Turing's 36 SMs make this proportionally costlier than on Ada. |
| Depth prepass (4) | 0.2 ms | Half of 1440p, few thousand objects. |
| Star rasterisation (5) | **5.5 ms** | The dominant cost. **10 M stars at 1440p.** Extrapolated from the published 22 M @ 4K in 8 ms on a 4070 Super, adjusted for Turing ALU count and the 6 MB L2. `[VERIFY]` in P0-S2. |
| Opaque + lighting (6–7) | 1.4 ms | |
| Accretion disks (8) | 0.4 ms | Only when a compact object is in frame. Geodesic raymarch is expensive on Turing — cap at 64 steps. |
| Volumetrics (9–11) | **2.2 ms** | Froxel grid 160×90×64 at 1/4 of 1440p, 1/8-rate temporal. **First to degrade.** |
| Transparent (12) | 0.3 ms | Overdraw risk from jets/comae — cap. |
| Picking (13) | 0.2 ms | 1/4-res, ID-only. |
| Glare + exposure (14–15) | 1.5 ms | Screen-space variant. The perspective-correct variant (~2.8 ms extrapolated) does not fit; make it an opt-in setting that drops the frame budget elsewhere. |
| Tonemap (16) | 0.5 ms | At 1440p. |
| **FSR2 upscale (16)** | **1.2 ms** | 1440p → 4K. This is the price of the whole strategy and it is worth it. |
| TAA resolve (17) | 0.4 ms | At 4K. `[MAY]` disable if FSR2's own temporal pass suffices. |
| Labels/UI (18) | 0.5 ms | ≤ 64 labels + panels. |
| **Total** | **14.9 ms** | **Over budget by 1.9 ms.** |

**The budget does not close on paper, and that is the honest finding.** Resolve it in this order (all are governor-controlled, §6.6):

1. Star budget 10 M → 8 M (−1.1 ms). Still the product's identity, so this is the last resort.
2. Volumetrics 1/4 → 1/6 res (−0.7 ms).
3. Glare pyramid depth 10 → 7 levels (−0.3 ms).
4. Accretion-disk geodesics off unless a compact object is the focus (−0.4 ms typical).
5. TAA resolve off where FSR2's temporal pass is sufficient (−0.4 ms).

With 1, 2, and 5 applied: **12.7 ms — inside budget with 8–10 M stars.** That is the realistic Titan RTX 4K60 configuration, and it is a genuinely good-looking frame: 8–10 M individually rasterised real stars with perceptual glare, volumetric dust, and correct HDR.

`[MUST]` The **adaptive quality governor (§6.6) is not optional polish — it is a correctness requirement.** Ship it in M1, not M7. On this card there is no static configuration that holds 60 fps across all views: the Galactic centre and a planetary surface differ by more than an order of magnitude in cost.

**Alternative configurations the user may prefer** (expose as presets):
- **`4K Quality`** — 2560×1440 internal + FSR2 Quality, 8–10 M stars, full volumetrics. 60 fps. *Recommended default.*
- **`4K Native`** — 3840×2160 internal, no upscaler, ~3 M stars, volumetrics at 1/8. 60 fps only in local views; 30–40 fps at the Galactic plane.
- **`1440p Native`** — no upscale, ~15 M stars, full volumetrics. 60 fps everywhere. *The best-looking stable configuration; seriously consider making this the default and letting the user choose 4K.*
- **`Unlocked`** — no frame cap, governor off, for benchmarking and screenshots.

### 6.3 VRAM budget (24 GB on the Titan RTX; ~22 GB usable after desktop compositing and driver overhead)

| Category | Budget | Detail |
|---|---|---|
| Star particle buffers (resident hot set) | 0.7 GB | 10 M × 32 B = 320 MB + 300 MB streaming double-buffer + slack. A 20 M hot set would be ~1.0 GB — still trivial here. |
| Octree node metadata | 0.05 GB | ~60 k nodes × 128 B. Permanently resident. |
| Render targets | 1.1 GB | **4K output:** HDR RGBA16F 66 MB, D32F 66 MB, FSR2 history ×2 ≈ 132 MB, TAA/post 66 MB. **1440p internal:** HDR 29 MB, star HDR 29 MB, depth 29 MB, motion RG16F 15 MB. Plus half-res depth 4 MB, picking R32UI ¼-res 2 MB, glare pyramid ~40 MB. |
| Volumetric froxel grid | 0.25 GB | 160×90×64 × (density+extinction RGBA8 37 MB, scattering RGBA16F 74 MB) + history. |
| Noise / LUT textures | 0.4 GB | 3× Worley 128³, 2× Perlin-Worley 128³, blue-noise 256², curl 128³, BRDF LUT, filter curves, blackbody LUT, Harre–Heller colour LUT. |
| 3D dust map texture | 1.0 GB | 512×512×256 R16F = 128 MB per map; up to 4 maps (local 3D, all-sky 3D, column density, model) + mips. |
| Imagery virtual-texture cache | 3.0 GB | DSS/2MASS/WISE/planetary albedo tiles. Raise to 5 GB freely — there is room. |
| Meshes, rings, shapes | 0.3 GB | Low-poly; spheres dominate. |
| Galaxy impostor atlas | 0.5 GB | ~5 000 galaxies as view-dependent impostors or oriented sprites. |
| **Subtotal** | **7.3 GB** | |
| **Headroom for streaming spikes** | **~14.7 GB** | Deliberately large — warp travel transiently doubles the star set. VRAM is *not* a constraint on this card; compute is. |

### 6.4 Culling cascade

Applied in this order, each stage reducing the input to the next:

1. **Persistent-set cull (CPU, per frame).** Reject octree nodes not in the resident set; issue stream requests for those that should be.
2. **Hierarchical frustum cull (GPU compute).** Node bounding sphere vs. six planes; a rejected node kills its whole subtree. ~60 k nodes → typically 2–8 k visible.
3. **Distance/solid-angle LOD cull (GPU compute).** The magnitude criterion from H2.2.
4. **Per-star frustum + magnitude cull (GPU compute).** Produces the compacted visible list.
5. **Occlusion cull (GPU, depth prepass).** Half-res depth from opaque geometry; stars and particles behind it are discarded in the rasteriser.
6. **Depth cull in dense media (GPU, per-froxel).** Optical-depth saturation cutoff (§H3).
7. **Screen-space density cull (GPU, per-tile).** Per-tile particle counters; a tile that already accumulated N contributions stops accepting more. This is the anti-"white-out" guard.
8. **Small-angle rejection.** Any object whose projected extent < 0.05 px and whose luminance < threshold is dropped entirely (it cannot contribute meaningfully and only costs rasteriser invocations).

### 6.5 Level-of-detail policy, per object class

| Class | LOD strategy |
|---|---|
| Stars (bulk) | Octree + magnitude criterion + software raster. Below ~0.5 px: single-tile splat. 0.5–4 px: analytic disc profile. > 4 px: billboard with limb-darkened photosphere shader, granulation noise, and (for the ~10⁴ largest/resolved) spot and plage features from real variability data where available. |
| Named/bright stars | Separate 128 B "extended" record with spectral type, variability class, multiplicity. Cross-fade from bulk representation over 2–3 px to avoid a visible pop. |
| Planets/moons | 3 sphere LODs (icosphere 3/5/7 subdivisions) + texture virtual-texture LOD. Rings: analytic alpha + real Cassini radial profile for Saturn. |
| Nebulae | Far: oriented sprite from real survey imagery (DSS/WISE/Hα). Mid: analytic envelope + volumetric. Near: full froxel volume with per-object detail noise. Cross-fade on a screen-size metric, never on distance alone. |
| Galaxies | Far: point + integrated magnitude. Mid: inclined-disk analytic profile with real Sérsic index, position angle, inclination, and axis ratio from LEDA/HyperLeda. Near: sprite/impostor from real imagery, plus resolved globular cluster system for M31. |
| Clusters (open/globular) | Far: single sprite with integrated magnitude and real half-light radius. Mid: resolved member stars (real member lists from Cantat-Gaudin et al. for open clusters; synthetic King/Plummer sampling calibrated to real concentration for globulars). Near: individual stars with real photometry. |
| SNR / PN | Shell geometry with real angular radius; volumetric emission with class-appropriate line ratios; expansion animated from the catalogued age and expansion velocity. |
| Small bodies | Below 1 px: single splat with correct magnitude and phase coefficient. Above: convex-hull shape model where PDS has one, else a triaxial ellipsoid from real a/b/c. |
| Jets | Analytic cone + helical noise, length from real data, opening angle from real data, Doppler-beamed brightness. |

`[MUST]` Every LOD transition uses a **dithered cross-fade over a screen-size window**, never a hard switch. Hard LOD pops are the single most visible defect in astronomical visualisers and they destroy the illusion of a continuous universe.

### 6.6 The adaptive quality governor (closed-loop dynamic LOD)

This is what "dynamic LOD" means here. It is a controller, not a slider.

- **Inputs** (from the async readback, 2 frames delayed): frame GPU time, star fragments rasterised, mean/max optical depth, visible node count, tile-saturation count, streaming backlog in bytes.
- **Outputs** (continuous, rate-limited): `m₀` (the magnitude LOD threshold), volumetric resolution scale, volumetric step count, temporal accumulation rate, glare pyramid depth, label budget, tile saturation cap.
- **Controller:** a PI controller on GPU frame time with target 12.0 ms (leaving 1.0 ms margin against the 13.0 ms budget). Anti-windup on the integrator. Output slew-limited to ±10 % per frame to avoid visible pumping. Hysteresis band of ±0.8 ms before the controller acts at all.
- **Priority order for degradation** (degrade first → last): volumetric temporal rate → volumetric resolution → label budget → glare pyramid depth → volumetric step count → star magnitude threshold `m₀`. Stars degrade *last* because star density is the product's identity.
- **Escalation on sustained overload** (> 3 s above target): step the quality preset down one notch and surface a diegetic, dismissible notice. Never silently drop a preset without telling the user.
- `[MUST]` Every knob the governor moves is also a user setting with a hard ceiling/floor, and the governor's live values are visible in the debug overlay. An unobservable adaptive system cannot be debugged.

---

## 7. STAR AND OBJECT RENDERING — DETAIL

### 7.1 Photometry to pixels

The chain, and it must be exactly this or colours will be wrong:

1. **Apparent magnitude** in the source's native band (Gaia G, V, J, H, K…).
2. **Extinction correction.** Apply 3D dust extinction along the line of sight (§7.5) to get intrinsic magnitude. *Both* values must be retained — the app displays "as observed" by default and offers "extinction-corrected" as a science mode. Never silently correct.
3. **Absolute magnitude** `M = m − 5·log₁₀(d_pc) + 5`, with `d` from parallax. Where parallax is negative or has `parallax_over_error < 5`, **do not invert the parallax** — use a Bayesian distance prior (Bailer-Jones et al. `distance` table, which Gaia publishes) or fall back to photometric distance. Naive `1/parallax` at low S/N produces spectacular artefacts (stars flung to 100 kpc) and is the most common bug in amateur Gaia visualisers. `[MUST]`
4. **Colour.** From `bp_rp` where available; else from `B−V` via a transform; else from spectral type; else blackbody from `T_eff`. Convert to linear sRGB via the **Harre & Heller (2021) digital colour codes of stars** look-up table — this is a published, empirically-derived mapping and is the right answer, not `blackbody → RGB` naïvely (which over-saturates blue stars).
5. **Luminance.** `L = L₀ · 10^(−0.4·M)` scaled into scene radiance. Gaia Sky's pseudo-size approach (`α = atan(p/d)`, with `p` derived from absolute magnitude) is the reference implementation and is documented publicly.
6. **Limb darkening** for resolved discs: a 2-coefficient Claret law per spectral class. Cheap, and it is what makes a resolved star look like a star rather than a sticker.

`[MUST]` A validation test: render 20 reference stars of known spectral type and assert the on-screen chromaticity is within ΔE₂₀₀₀ < 3 of the Harre–Heller published value.

### 7.2 The star profile

For a resolved star, the intensity profile is not a Gaussian — it is a limb-darkened disc convolved with the PSF. Implement:
- Core: analytic limb-darkened disc.
- Halo: the diffraction/seeing PSF approximated by the glare pyramid (§7.4), which is why glare is a *physical* component here rather than a bloom effect.
- `[MAY]` Diffraction spikes for the brightest ~50 stars, with a user toggle, clearly labelled as an instrument artefact (it is — real eyes and real telescopes without a secondary spider do not produce them). Off by default under D9.

### 7.3 Software rasteriser — required behaviour

Per the Eurographics 2025 result. Specification:
- Compute shader, one workgroup per 8×8 screen tile.
- Per tile: load the tile's star list (from a tile-binning pass over the compacted visible list), then for each star compute its footprint and atomically accumulate `radiance × coverage` into the HDR target.
- **Perspective-correct solid angle.** The paper's central insight: a single-pixel star at the screen edge covers a *smaller* solid angle than one at the centre, so edge stars must be drawn proportionally brighter to preserve total luminance. Skipping this makes wide-FOV views look wrong and is invisible at narrow FOV — which is exactly why it gets missed. `[MUST]` implement it.
- Coverage: analytic disc/pixel-overlap area, not a point sample. This is what kills the shimmer.
- Depth test against the half-res depth prepass (upsampled conservatively) for occlusion.
- Atomic float add if available; otherwise per-tile u32 fixed-point accumulation with a frame-global luminance normalisation factor (the paper's approach).

### 7.4 Glare / bloom — the perceptual pyramid

Do **not** use a plain Gaussian bloom. Use the Krawczyk–Myszkowski–Seidel perceptual approach as implemented in the 2025 paper:
- Build a mip pyramid from the HDR target at half render resolution.
- Each level is a progressively wider Gaussian (the halving resolution does the widening for free).
- Composite with weight `1/2ⁱ` and **bicubic** texture filtering (Sigg & Hadwiger, *Fast Third-Order Texture Filtering*, GPU Gems 2).
- The resulting sum converges to `c/θ²` — the empirical form of human veiling luminance — with < 1 % relative error for 0.2° ≤ θ ≤ 90° at n = 10 levels.
- Two variants: **screen-space decomposed** (fast, 0.74 ms @4K on a 4070 Super, but glare is circular and wrong at wide FOV) and **view-space asymmetric** (perspectively correct, 1.38 ms @4K). Default to screen-space; expose the asymmetric variant as a quality setting.
- Mix ratio between the raw target and the glare result is a user setting ("glare strength"), default 20 % of luminance contributed by glare per the paper's recommendation.

This one decision is responsible for more of the "beautifully rendered" quality than any other single item in the plan. Budget the time for it.

### 7.5 Dust extinction

- Sample the 3D dust density field (§H3) along the ray from the Sun (or camera, in local mode) to each star, integrated to get `A_V`.
- Apply the **Fitzpatrick (1999)** or **Cardelli–Clayton–Mathis (1989)** extinction law with `R_V = 3.1` default and a per-region `R_V` override where catalogued.
- For stars beyond the 3D map's range (1.25 kpc for Edenhofer; 2.8 kpc for Dharmawardena), fall back to a **statistical Galactic extinction model**: a double-exponential dust disk (scale height ~125 pc, scale length ~3 kpc) integrated along the ray. This is not a fudge — it is the standard model, and it is what makes the Galactic plane correctly opaque at low latitudes.
- `[MUST]` Extinction is applied at *ETL time* for the bulk star set (baked into the quantised colour/magnitude) and at *runtime* for the local 3D region where the camera moves enough for the ray geometry to change. Baking is what makes 20 M stars affordable.
- Validation: the extinction-corrected colour–magnitude diagram of a 200 pc sphere must show a clean main sequence; the uncorrected one must show a reddened, smeared sequence. That visual difference *is* the test.

### 7.6 Compact objects and accretion

Given D9 (science-faithful), do these properly or not at all:

- **Accretion disk around a black hole.** A raymarched thin disk in the equatorial plane with a relativistic transfer function. Full geodesic integration is affordable *only* because there are at most a handful of such objects on screen. Approach: integrate null geodesics in Schwarzschild (or Kerr for the ~30 objects with catalogued spin) using a fixed-step RK4 in the raymarcher, 64–128 steps, with early exit. Doppler shift and beaming applied per emit-point. Gravitational lensing of the background starfield comes free if the disk ray also samples the star buffer — `[MAY]`, expensive.
  This is the "Interstellar shot" and it is worth doing: Sgr A* and Cygnus X-1 are both in the curated lists (§12.3).
- **Neutron stars / pulsars.** Rotating magnetic dipole: two opposing conical beams along the magnetic axis, offset from the spin axis by the catalogued α. Beam sweeps → observable pulsation only if the geometry crosses the line of sight, which is *correct behaviour* and should be surfaced in the info panel ("beaming: does not intersect your line of sight"). Period from the ATNF catalogue; spin-down from the same.
- **Magnetars.** Same, plus an X-ray/gamma glow rendered as an emission sprite with a hard spectrum. `[MAY]` a false-colour survey mode where they are prominent.
- **Supernova remnants.** Expanding shell, radius = catalogued radius + expansion velocity × (epoch − catalogued age). Line emission from §H3. Real morphologies (shell, filled, composite) from SNRcat.
- **Planetary nebulae.** Bipolar/elliptical/spherical morphology classes with real axis ratios; central star rendered with its real (very hot, very faint) temperature; ionisation-stratified line emission — [O III] dominant in the inner shell, Hα and [N II] in the outer. This stratification is real, is visible in every good PN image, and is maybe 30 lines of shader.

### 7.7 The "enhanced visibility" gain mode

D9 says science-faithful. Reality: much of the ISM is far too faint to see on an SDR display at a physically correct exposure, and a user who zooms into a real molecular cloud will see *nothing* and conclude the app is broken.

Resolution — two explicit, labelled modes:

- **`Observed`** (default): physically calibrated radiance, auto-exposure, no gain. What a telescope would record.
- **`Enhanced`**: a non-physical, user-controlled gain applied *only* to faint extended emission, with a persistent, unmissable on-screen badge reading "ENHANCED VISIBILITY — NOT TO SCALE". Separate sliders for dust, ionised gas, and synchrotron.

This is exactly what real astronomy software and real survey imagery do (colour-composite "false colour" images are the norm), and labelling it preserves scientific honesty. It also lets the app be beautiful without lying. `[MUST]` the badge; `[MUST NOT]` silently enable Enhanced.

---

## 8. DATA LAYER

### 8.1 The split: Python offline ETL, Rust online runtime

`[MUST]` **All catalog ingestion, normalisation, cross-matching, classification, octree construction, and quantisation happens in Python, offline.** The Rust app reads only pre-baked binary artifacts.

Rationale, which the build session should not relitigate: astronomy's tooling (astroquery, astropy, healpy, TOPCAT/STILTS, CDS X-Match) is Python/Java and is a decade ahead of anything in Rust. Reimplementing VOTable parsing, HEALPix, and TAP clients in Rust would consume the entire project budget for zero user-visible benefit. The Rust side gets a clean, stable, zero-copy binary contract instead.

Exception: `galaxia-data` in Rust *does* implement octree traversal, streaming, resident-set management, and (optionally) runtime cone search against the baked index. That is runtime work and belongs in Rust.

### 8.2 ETL pipeline stages

Every stage is idempotent, resumable, and writes a checksummed artifact plus a lineage record. `[MUST]`

```
acquire   →  raw/<source>/<version>/          (download, verify checksum, record provenance)
ingest    →  norm/<source>.parquet            (DuckDB/Polars; map to common schema)
astrom    →  norm/<source>.astrom.parquet     (ICRS→galactic, parallax→distance w/ Bailer-Jones,
                                               absolute mag, colour transforms, extinction)
classify  →  norm/<source>.classified.parquet (taxonomy IDs + render archetype, §11.2)
xmatch    →  identity.parquet                 (dedupe across catalogs → canonical object ID)
enrich    →  objects.parquet                  (names, aliases, descriptions, citations, links)
octree    →  build/tree.bin + build/leaf_*.bin (Morton sort, LOD pyramid)
quantize  →  build/stars_tier{0,1,2}.bin      (fixed-stride packed GPU-ready records)
index     →  build/search.idx                 (trie + payload for §12.1)
imagery   →  build/tiles/<survey>/z<0..14>/   (HiPS-reprojected, WebP/BC7 tiles)
validate  →  reports/*.json                   (§17 differential astrometry + statistics)
package   →  data/<tier>/ + MANIFEST.toml     (tiering, sizes, checksums, licenses, credits)
```

**Versioning.** `[MUST]` Every source adapter implements:

```
trait SourceAdapter:
    source_id() -> &'static str            # "gaia"
    version()   -> SourceVersion           # Dr3 | Dr4 | ...
    schema_map() -> ColumnMapping          # source column → common field
    quirks()    -> QuirkSet                # e.g. negative parallaxes, duplicate source_ids,
                                           #      epoch differences, bandpass mismatches
```

The common schema is **stable**; adapters change. This is the entire D7 mitigation — DR4 support is a new adapter plus a re-run, not a rewrite. Budget 2 weeks for the DR4 adapter in M8; `[VERIFY]` against the DR4 pre-release astrometric time-series data that ESA has already published, and the published draft DR4 data model.

**Provenance.** `[MUST]` Every object and every star carries:
- `provenance: u8` — `Real` (catalogued, with source ID), `Derived` (computed from real data, e.g. absolute magnitude), `Synthesized` (procedural, with model version + seed), `Modeled` (statistical model, e.g. extinction beyond the map range).
- `source_ids: [u32; N]` — index into the source registry.

The UI *must* surface this. A synthesized star's info panel says "Synthesized — statistically consistent with the Besançon model at this location; not a catalogued object." Under D9 this is non-negotiable: silently presenting procedural content as real data is the one thing that would make this app dishonest.

### 8.3 Common object schema

One canonical record for all ~10⁶–10⁷ first-class objects:

| Field | Type | Notes |
|---|---|---|
| `object_id` | `u64` | Stable across rebuilds (hash of primary designation + source). `[MUST]` — user bookmarks reference this. |
| `ra`, `dec` | `f64` | ICRS J2000, degrees → converted to AU f64 at build time. |
| `epoch` | `f64` | JD. Gaia DR3 = 2016.0; DR4 will differ. Per-source. |
| `dist_pc`, `dist_err` | `f32` | Bailer-Jones geometric/EDR distance, not `1/parallax`. |
| `pm_ra`, `pm_dec`, `rv` | `f32` | mas/yr, km/s. For proper-motion animation. |
| `mags[f32; 8]` | | G, BP, RP, V, B, J, H, K — NaN where absent. |
| `taxonomy_id` | `u32` | §11.1 tree index. |
| `archetype_id` | `u16` | Which renderer path. |
| `provenance` | `u8` | §8.2. |
| `radius_au`, `mass_msol`, `lum_lsol`, `teff_k` | `f32` | NaN where unknown. Never fabricate. |
| `name_primary` | `str` offset | |
| `aliases` | `str` offset range | All designations. |
| `desc` | `str` offset | 1–3 sentences, sourced. |
| `refs` | `str` offset | Bibcode/DOI list. |
| `extra` | blob offset | Archetype-specific: orbital elements, Sérsic params, jet length, period, etc. |

`[MUST]` **NaN is a valid and common value.** Unknown ≠ zero. Every consumer must handle it, and the UI must render "unknown" rather than "0". A `radius: 0.0` planet is a bug that will silently render invisible objects for months before anyone notices.

### 8.4 Star record format (GPU)

Two tables, hot and cold:

**Hot (streamed, one per octree leaf, 32 B fixed stride, `#[repr(C)]`):**

| Offset | Size | Field | Encoding |
|---|---|---|---|
| 0 | 6 | `pos` | 3 × `i16`, offset from node centre in node half-extent units → ±32767 gives ~1/65534 of the node extent. For a 10 pc node that is 1.5 × 10⁻⁴ pc ≈ 3 × 10⁴ km — plenty. |
| 6 | 2 | `flags` | variability class, multiplicity bit, provenance, resolved-capable bit |
| 8 | 4 | `phot` | 2 × `u8` magnitude quantised (0.02 mag steps over a 5.1 mag window), 2 × `u8` colour indices |
| 12 | 6 | `pm` | 3 × `i16` proper motion, quantised |
| 18 | 2 | `ext` | `A_V` quantised u8 + `R_V` quantised u8 |
| 20 | 4 | `cold_idx` | index into the cold table, or `u32::MAX` |
| 24 | 8 | — | padding / future use |

**Cold (~10⁶ records, 128 B, resident for the bright/named subset only):** spectral type string index, full multi-band photometry, `T_eff`, `[Fe/H]`, `log g`, mass/radius/luminosity with errors, variability period and amplitude, binary orbital elements, cross-identifications.

**Why quantise this hard:** 32 B × 3 × 10⁸ stars = 9.6 GB on disk; the same at 20 B would be 6 GB. The quantisation above is chosen so that *no visible artefact* results at any zoom level the octree can reach — the node-relative i16 trick is what makes it work, and it is the reason this fits.

**Octree node record (128 B):**

```
centre: f64[3] (AU)      half_extent: f32      level: u8      flags: u8
n_stars: u32             n_descendants: u32
brightest_M: f32         faintest_M: f32       centroid_offset: f32[3]
children: u32[8]         payload_offset: u64   payload_bytes: u32
```

**Node count check:** 1.5 × 10⁹ stars ÷ 10⁵ per leaf = 15 000 leaves, ~30 000 nodes, 3.8 MB metadata. 3 × 10⁸ ÷ 10⁵ = 3 000 leaves, ~6 000 nodes, 768 KB. Both trivially resident. `[MUST]` enforce a max depth (~24) so sparse regions do not produce degenerate chains.

### 8.4.1 THE 200 GB DISK CONSTRAINT — mandatory ETL design rules

The build machine has **200 GB of free NVMe**. This is the tightest resource in the entire project and it dictates the ETL design. Read this before writing any ingest code.

**What does not fit:**

| Thing | Size | Verdict |
|---|---|---|
| Full Gaia DR3 `source` CSV dump | ~1 TB compressed / ~2 TB unpacked | **Impossible.** Never attempt it. |
| All-sky raw extract at G < 20 in one pass | ~400–600 GB | **Impossible.** |
| All-sky raw extract at G < 17 in one pass | ~120 GB | Possible but leaves nothing for output. **Don't.** |
| Orion-region raw CSV (18° radius, G < 20, ~35 M rows) | ~14 GB | **Fine.** This is the prototype. |
| Orion-region baked artifact | ~1.1 GB | Fine. |
| Tier 1 all-sky baked (2 × 10⁸ stars @ 32 B) | ~6.4 GB | Fine — *baked output is small; raw is the problem.* |

**The key insight: the baked output is ~30–60× smaller than the raw input.** 32 bytes per star, quantised and zstd-compressed, versus ~400 bytes per star in Gaia CSV. The constraint is entirely about **peak raw+intermediate residency**, not about final data size.

**`[MUST]` Mandatory ETL rules under this constraint:**

1. **Streaming-partitioned ingest.** Never materialise a full raw extract. Partition the sky (HEALPix Nside = 8 → 768 pixels, or RA bands of 4 h), and for each partition: `query → normalise → astrom → quantise → append to the global octree input → DELETE the raw partition`. Peak residency = one partition's raw (~2–4 GB) + intermediates.
2. **Peak-disk budget: 60 GB during a full all-sky build**, leaving 140 GB for the OS, the workspace, the repo, shader caches, and headroom. `[MUST]` the ETL enforces this itself: before each partition it checks free space and refuses to start if `free < 40 GB`.
3. **Zstd-compress every intermediate.** Parquet + zstd level 12 for the `norm/` and `astrom/` stages. Never store uncompressed intermediates.
4. **Two-phase octree build.** Phase 1 streams all partitions into a single Morton-sorted external file using DuckDB's out-of-core sort (96 GB RAM helps enormously here — the sort will mostly stay in memory). Phase 2 packs leaves. Neither phase needs the raw data.
5. **A `df` gate in CI and in the ETL entrypoint.** Fail loudly, early, with a clear message. Running out of disk mid-build produces corrupt artifacts that look like data bugs and cost days to diagnose.
6. **`--keep-raw` is off by default** and requires an explicit flag plus a free-space check.
7. **Prototype region data is committed-adjacent.** The ~1.1 GB Orion baked artifact should be small enough to keep permanently and even to back up. Treat it as the golden reference dataset for tests.

**Consequence for tiering:** the Full tier (~300 GB baked) **is not installable on this machine** and must be deferred or built elsewhere. See §8.5 for the revised tier definitions. This is not a loss for the prototype — Orion at G < 20 exercises every code path the Full tier would.

### 8.5 Tiering (D8, revised for 200 GB)

| Tier | Baked size | Peak build disk | Star content | Non-star content |
|---|---|---|---|---|
| **T0 Orion** *(prototype)* | **~1.5 GB** | ~20 GB | Real Gaia DR3, 18° radius around (83.8°, −5.4°), G < 20 → ~35 M stars | Messier + Sharpless + Lynds + Barnard for the region; Edenhofer 3D dust (the region is well inside 1.25 kpc); HYG named stars; Solar System planets + major moons |
| **T1 Core** | **~15 GB** | ~60 GB | All real stars G < 15 all-sky (~3 × 10⁷) + everything within 100 pc + all named (~1.2 × 10⁵) + synthesized envelope | Full 10⁶-object catalog, complete taxonomy, 3D dust to 1.25 kpc, Solar System albedo maps, ~2 000 DSS tiles for the Messier/Caldwell set |
| **T2 Standard** | **~55 GB** | ~90 GB (requires T1 output temporarily offloaded) | All real stars G < 19 all-sky (~2 × 10⁸) + synthesized envelope | + all-sky dust to 2.8 kpc, + 2MASS/WISE all-sky tiles, + ~5 000 galaxy impostors, + 10⁴ small-body orbits |
| **T3 Full** | ~300 GB | — | All real stars G < 20.7 (~8 × 10⁸) | **NOT INSTALLABLE on this machine.** Defer indefinitely, or build to external/scratch storage. The prototype loses nothing by skipping it. |

`[MUST]` **Build order is strictly T0 → T1 → T2, and each is built only when the previous one is in daily use.** Do not build T2 speculatively. T0 alone is sufficient for the entire prototype phase and for most of M1–M5.

**Budget check against 200 GB free:** T0 (1.5) + T1 (15) + T2 (55) = **71.5 GB of installed data**, plus ~30 GB of repo/workspace/shader caches/build artifacts, plus ~60 GB of transient ETL working space during a build → **~160 GB peak, ~40 GB headroom.** Workable, but tight enough that the `df` gate (rule 5 above) is not optional.

`[MUST]` The runtime queries a `MANIFEST.toml` at startup and exposes `data_caps()` — every subsystem asks "do I have X?" and degrades gracefully. **No tier may crash, and no tier may silently show wrong data.** A region that is not installed shows as synthesized-with-a-badge, or is marked "not installed" in the info panel.

### 8.6 Streaming and the resident-set manager

- Async I/O via `tokio` with a bounded channel; 2–4 reader threads; `zstd`-compressed leaf payloads decompressed on the reader pool.
- **Morton-order sort within each leaf** so that a partially-read leaf still yields a spatially coherent subset. This is what makes progressive refinement look right instead of random.
- LRU eviction with a *pinning* set: the current focus object's neighbourhood and the coarse LOD levels (0–4) are never evicted.
- Priority queue for requests: `priority = solid_angle × brightness_factor / (1 + distance_to_target)`. Warp travel sets `distance_to_target` dominant.
- `[MUST]` Never block on I/O in the render thread. If a leaf is not resident, render the parent's representative points instead — the octree's cumulative statistics make this correct-by-construction.
- `[MUST]` Surface the streaming backlog in the debug overlay (bytes/s, queue depth, eviction rate). Streaming starvation is the hardest bug class to diagnose without it.

### 8.7 Procedural synthesis (the "hybrid" half of D2)

- **Model:** a Besançon-style population synthesis (Robin et al. 2003) — thin disk (scale length 2.5 kpc, scale height age-dependent), thick disk, bar, bulge (boxy/peanut), stellar halo, plus spiral-arm modulation from a Vallée or Hou & Han logarithmic-arm model. Implement as a **rejection sampler over a Galactic density function** with an IMF (Kroupa or Chabrier) and an age/metallicity distribution.
- **Seeding:** `[MUST]` the seed is a pure function of the octree node ID: `seed = hash(model_version, node_id)`. Regenerating a node always yields the same stars. Without this, stars flicker and change identity as the camera moves, which is immediately and deeply wrong-feeling.
- **Calibration and validation:** `[MUST]` in the overlap region (where both real and synthesized stars exist), compare the synthesized population against real Gaia counts in cells of `(l, b, m)`. Acceptance: within 10 % in star counts and within 0.2 mag in the mean colour per magnitude bin, over ≥ 10⁴ cells spanning the plane, the arms, inter-arm regions, and high latitude. If it fails, tune the model — **do not ship an uncalibrated synthesis**, because the seam where real data ends and synthesis begins will be visible as a change in star density, and it is the most likely thing to make the whole app feel fake.
- **Flagging:** every synthesized star carries `provenance = Synthesized`. The legend and the search have a "real only" filter. A debug mode renders synthesized stars in a distinct colour so the seam is inspectable.
- **What synthesis must NOT do:** invent named objects, invent catalog designations, or place synthesized stars where a real catalog says there are none. It fills *volume*, not *identity*.

---

## 9. SIMULATION AND TIME

### 9.1 Time system

- Internal clock: `f64` days since J2000.0 TDB.
- Scales: TDB, TT, TAI, UTC (via a leap-second table shipped as data and updated by the ETL — `[VERIFY]` the table's last entry each run and warn if stale, since IERS adds leap seconds irregularly).
- Rate range: `0` (paused), `1×` real-time, and continuous log-scaled warp from 1 s/s to 10⁶ yr/s. Plus discrete jumps.
- **Sub-stepping.** `[MUST]` At high warp rates, do not just advance the clock — integrate in sub-steps so that (a) fast-orbiting inner planets do not alias, and (b) transient events are not skipped over. Sub-step count = `ceil(dt / min_period_of_interest / 8)`. Cap at 4096 sub-steps/frame; above that, switch to "epoch snap" mode where only the endpoints are computed and the user is told events may have been missed.
- **Discrete epoch jumps:** J2000.0, Gaia DR3 reference epoch (2016.0), Gaia DR4 epoch, "now", 1054 (SN 1006/Crab), 1572 (Tycho), 1604 (Kepler), 1987A, 2017 (GW170817), +1000 yr, +1 Myr, +4.5 Gyr (Milky Way–Andromeda encounter, per van der Marel et al.), −100 Myr.
- **Precession/nutation:** IAU 2006 precession + IAU 2000B nutation. Required for alt-az mode and for the "sky as seen from Earth at date X" correctness.
- **Aberration and light-time:** §9.3.

### 9.2 Ephemeris (D4: Keplerian + perturbations)

- **Planets and Pluto:** VSOP87 (planetary theory, truncated) for the eight planets; a truncated Meeus series for Pluto. Accuracy: ~1 arcsecond over ±2000 yr, far better than the arcminute target. Use the `celestial-ephemeris` crate if it proves complete (`[VERIFY]` — it claims VSOP2013 + ELP/MPP02 lunar theory + SPK support); otherwise vendor the truncated VSOP87 coefficient tables (they are public domain, published in the *Astronomical Almanac* and in Meeus).
- **Moon:** ELP2000-82B truncated, or Meeus chapter 47. Target: ~10 arcsec.
- **Major moons (Galilean, Titan, Triton, Charon, the major Uranian/Neptunian set):** catalogued Keplerian elements with linear and quadratic rates. Accuracy ~10⁻³ rad — visually perfect at every zoom level we support.
- **Small bodies (~10⁴):** JPL SBDB / MPCORB elements with osculating epoch, propagated by universal-variable Kepler. Include the `H`, `G` magnitude parameters so brightness is right, and the phase-curve model (HG or HG1G2) — a comet or asteroid at the wrong phase angle is visibly wrong.
- **Comets:** two-tail model. Dust tail: syndyne/synchrone approximation (a few hundred test particles, cheap). Ion tail: anti-sunward with a real curvature from solar-wind velocity. Activity driven by `r_h` (heliocentric distance) via a catalogued or default `Q ∝ r_h^−n` law. This is one of the more visually rewarding items in the whole plan.
- **Exoplanet systems (~6 000):** Keplerian elements from the NASA Exoplanet Archive (`pl_orbe`, `pl_orbi`, `pl_orbper`, `pl_orbsmax`, `pl_orbeccen`, `pl_trandur`, `pl_tranmid`). Where elements are missing, use the transit-derived minimum set and mark the orbit as "partially constrained" — do **not** invent an eccentricity.
- **Galactic motion:** the Sun on a circular-ish orbit at R₀ = 8.178 kpc (GRAVITY 2019; `[VERIFY]` current best) with Θ₀ ≈ 233 km/s, plus peculiar velocity (U, V, W) = (11.1, 12.24, 7.25) km/s (Schönrich et al. 2010). Other stars move by proper motion + radial velocity linearly (fine over ±10 kyr); over ±1 Myr, integrate them in a Galactic potential (MWPOT18 via a simple symplectic integrator, or McMillan 2017).
- **Andromeda:** the van der Marel et al. radial velocity and proper motion, integrated forward. At +4.5 Gyr the two galaxies should visibly interact. This is a headline feature for very little code.

**Escape hatch (documented in an ADR):** if arcminute accuracy proves insufficient for any use case, add a DE440 SPK reader behind the same `EphemerisProvider` trait. This is why the trait boundary matters.

### 9.3 Light-time correction — a headline feature

Every object is rendered where it *appears* to be, not where it *is*:

```
solve for τ such that  |observer(t) − object(t − τ)| = c·τ      (2–3 Newton iterations)
then apply annual aberration and (optionally) gravitational deflection
```

Then expose a **three-way toggle**:

| Mode | Meaning |
|---|---|
| `Apparent` | Where the object appears right now. Default. Correct for "what would I see". |
| `True (current epoch)` | Where the object actually is at the simulation's current time. |
| `Astro (J2000 mean)` | Mean place at J2000 — matches printed catalogs and lets the user cross-check against Stellarium. |

At galactic scale this becomes profound and is the honest implementation of "lightyears": looking toward the Galactic centre at 8 kpc, you see it as it was 26 000 years ago. With `Apparent` on, **every object is rendered at its own past epoch**, and the info panel shows both "distance: 8.1 kpc" and "lookback: 26 400 yr". `[MUST]` — this is the single most educational feature in the app and it costs almost nothing once τ-solving exists.

### 9.4 Motion and animation

- **Proper motion:** animate at the current warp rate. At 1000 yr/s the nearby stars visibly stream — this is a genuinely striking effect and is real.
- **Variable stars:** light curves from GCVS / AAVSO / Gaia `variability_summary` + `vari_cepheid` + `vari_rrlyrae`. Render as time-varying magnitude using the catalogued period and amplitude (Fourier series for Cepheids/RR Lyrae where coefficients are published; sinusoidal otherwise). ~10⁵ objects — do this only for the extended/cold table.
- **Pulsars:** period + period-derivative from ATNF. Render the beam sweep; *do not* attempt to show the pulse rate at real-time scale (it is invisible), but show it in the info panel with a phase indicator.
- **Transients:** supernovae from the Open Supernova Catalog with real light curves — so at epoch 1054, the Crab's progenitor is a bright new star that fades on the real timescale. GRBs from the Open GRB Catalog as brief directional flashes with real durations and sky positions. FRBs and GW events similarly. `[MUST]` these are epoch-driven, not random.
- **Solar activity:** `[MAY]` sunspot count from SILSO driving a subtle photospheric texture and corona brightness. Nice, cheap, real.

---

## 10. CAMERA AND NAVIGATION

### 10.1 The camera rig

- **State:** position (f64, in the current render frame's parent), orientation (quaternion f64), focus (`Option<BodyId>`), FOV (default 55°, range 5°–120°), roll.
- **Speed model:** `[MUST]` exponential, not linear. `speed = base × 10^(k × throttle)` where `throttle ∈ [−1, 1]` maps to ~10 km/s … ~100 kpc/s. Across 20 OoM there is no other workable mapping. Scroll wheel adjusts `k` (the octave), not the speed directly.
- **Damping:** critically-damped spring on both position and orientation (`ζ = 1.0`), with a **jerk limit** so that no input produces an instantaneous acceleration. Time constant ~0.18 s for rotation, ~0.35 s for translation. This is what "smooth camera movement" means concretely; without the jerk limit, high-precision mice produce visible snap.
- **Rotation pivot:** orbits around the focus if set, otherwise around a point at `0.3 × distance_to_nearest_geometry`. Recomputed continuously, not on click.
- **Focus modes:** `Free`, `Orbit(body)`, `Chase(body)` (camera maintains a fixed body-relative offset), `Lock(body)` (body pinned to screen centre, camera co-moves). `[MUST]` mode transitions are animated over ~0.6 s, never instant.
- **Travel-to:** a **two-phase manoeuvre** — (1) a logarithmic spiral out-and-over that raises the camera to a vantage where both origin and destination are in frame, (2) a descent into an orbit of the target at a distance chosen from the target's radius (`4 × radius` for a planet, `2.5 × half-light radius` for a cluster, `3 × apparent diameter match` for a galaxy). Duration scaled by `log₁₀(distance_ratio)` clamped to [0.8 s, 6 s], with an ease-in-out quintic. During travel, time may optionally advance to match (a "real transit" mode) — default off.
  `[MUST]` Travel is interruptible by any input, and interruption hands off smoothly to manual control with velocity continuity (no stop-and-restart).
- **Auto near/far:** derived (§5.4), never manual.
- **Auto-exposure during travel:** the exposure controller must have a *velocity-aware* mode — during a warp the scene luminance changes by orders of magnitude in a second, and a naive auto-exposure strobes. Fix: during travel, drive exposure from the *predicted* luminance at the destination (computed from the octree's cumulative statistics) rather than from the current frame's histogram, then hand back to the histogram controller on arrival.

### 10.2 Input

- Mouse: LMB select, RMB radial menu, MMB pan, wheel zoom-octave, shift+wheel FOV, WASD translate, QE roll, RF vertical, space brake, tab focus-next, `/` search, `L` legend, `T` time panel, `F` frame focus, `Esc` release.
- `[MAY]` Gamepad: left stick translate, right stick look, triggers zoom-octave, A select, B back, X radial, Y focus. Worth ~2 days and greatly improves the "navigable" feel.
- `[MUST]` All bindings remappable and persisted. All actions go through an `Action` enum — no raw key handling in subsystems.

### 10.3 The diegetic label system (D10) — the hard UI problem

Labels across 25 OoM of zoom is the crux. Specification:

- **Budget:** hard cap of 64 labels + 4 panels + 1 legend + 1 time dial + 1 scale bar. Never more. The budget is governor-controllable (§6.6).
- **Importance score:** `score = w₁·apparent_brightness_norm + w₂·is_named + w₃·proximity_to_screen_center + w₄·is_selected_or_hovered + w₅·catalog_significance − w₆·occlusion_penalty`. Take the top 64, then declutter.
- **Declutter:** greedy screen-space bin packing in score order — reject a label whose AABB intersects an already-placed one, unless it is the selected object (selected always wins, and losers get a leader line to a free position or are dropped).
- **Scale invariance:** labels are **screen-constant size** (world-anchored, screen-scaled). A label that scales with zoom is unreadable at 90 % of zoom levels.
- **Occlusion:** depth-test the anchor against the depth buffer; occluded labels either fade to 30 % with a dashed leader (for named/selected objects) or vanish (for everything else).
- **Velocity fade:** fade all non-selected labels out above a camera-angular-velocity threshold. During warp, the screen should be clean.
- **Level-of-abstraction:** what gets labelled changes with zoom regime — at > 20 kpc, label spiral arms, the bar, the Galactic centre, and satellite galaxies; at 100 pc–1 kpc, label nebulae, clusters, and named stars; at < 1 pc, label planets and moons; at < 0.01 pc, label individual bodies. `[MUST]` the transition between regimes is a cross-fade, not a swap.
- **Text rendering:** GPU SDF glyph atlas (generate offline from a font via `cosmic-text` for shaping → atlas bake into `assets/`), instanced camera-facing quads, one draw call for all labels, per-glyph outline for legibility against bright starfields. Sub-pixel positioning with a 4×4 SDF supersample. Do **not** use egui for this.
- **Info panels:** world-anchored cards that billboard to the camera, placed to avoid the object's own disc and to avoid screen edges, with a leader line. Content: identity (name, designations, constellation), classification chip, astrometry, photometry, physical parameters (with error bars, and "unknown" where unknown), a distance/lookback pair, provenance badge, references, and 3–6 contextual actions (Focus, Travel to, Add to tour list, Show in survey X, Show light curve, Show neighbours).
- **Radial menu:** context-sensitive arcs around the cursor — Navigate (Focus/Orbit/Chase/Lock/Travel), Observe (survey band selection, exposure, enhanced visibility), Inspect (info panel, light curve, orbit plot, neighbours within N pc), Filter (isolate class, hide class, show only real). Radial menus are drawn with the same GPU 2D path as labels.
- **egui exception:** the settings overlay, debug overlay, and first-run wizard use egui. They are framed diegetically as a "cockpit systems panel" so the fiction holds. This is a deliberate pragmatism: rebuilding a settings tree as a custom diegetic widget is 3 weeks of work with zero experiential payoff.

---

## 11. OBJECT MODEL, CLASSIFICATION, AND LEGEND

### 11.1 Taxonomy

A single rooted tree in `galaxia-astro::taxonomy`, with stable `u32` IDs (never reused, never renumbered — user configs and bookmarks reference them). Defined in a data file (`taxonomy.toml`) so it can grow without recompiling. Structure (4 levels max; the full enumeration lives in `docs/taxonomy.md`):

```
Universe
├── Stellar
│   ├── ByLuminosityClass  (O B A F G K M × I II III IV V VII; subdwarfs; carbon/S stars; Wolf–Rayet;
│   │                        blue/yellow/red hypergiants; T Tauri; Herbig Ae/Be)
│   ├── ByEvolutionaryState (pre-main-sequence, main-sequence, subgiant, giant, AGB, supergiant,
│   │                        post-AGB, remnant)
│   ├── CompactRemnants     (white dwarf [DA DB DQ DZ DC]; neutron star; pulsar [radio, X-ray, gamma,
│   │                        millisecond]; magnetar; stellar-mass BH; intermediate-mass BH)
│   ├── Substellar          (brown dwarf L/T/Y; free-floating planetary-mass)
│   ├── Variable            (intrinsic: Cepheid, RR Lyrae, Mira, semiregular, long-period, Delta Sct,
│   │                        Beta Cep, LBV, eruptive; extrinsic: eclipsing binary [EA EB EW],
│   │                        rotating, pulsar-host)
│   └── Multiple            (visual binary; astrometric binary; spectroscopic binary; hierarchical;
│                            common envelope; contact binary)
├── NonStellar phenomena    (accretion disk; relativistic jet; pulsar wind nebula; bow shock;
│                            proplyd; protoplanetary disk; debris disk; Dyson-candidate [tabulated,
│                            flagged speculative])
├── StellarSystems          (open cluster; globular cluster; association; moving group; stellar stream;
│                            tidal tail; hypervelocity star)
├── InterstellarMedium
│   ├── Nebulae             (emission/H II; reflection; dark; planetary [bipolar, elliptical, spherical,
│   │                        quadrupolar]; SNR [shell, filled, composite]; nova remnant)
│   ├── Clouds              (molecular cloud; GMC; Bok globule; cometary globule; infrared-dark cloud;
│   │                        cirrus; CO cloud)
│   └── LargeScale          (superbubble; Local Bubble; Loop I; Magellanic Stream; Galactic fountain)
├── Galactic
│   ├── Structure           (spiral arm; bar; bulge; thin disk; thick disk; halo; stellar bridge)
│   ├── Components          (globular cluster system; dust ring; molecular ring; warp; flare)
│   └── Circumgalactic      (CGM; halo cloud; satellite galaxy [dSph, dIrr, dE])
├── SolarSystem
│   ├── Planets             (terrestrial; gas giant; ice giant)
│   ├── DwarfPlanets
│   ├── Moons               (regular; irregular; captured; co-orbital/Trojan; shepherd)
│   ├── SmallBodies         (asteroid [main belt, family-tagged, near-Earth, Apollo/Amor/Aten, Trojan,
│   │                        Hilda, cybele]; centaur; comet [short-period, long-period, Halley-type,
│   │                        Jupiter-family]; TNO [classical, resonant, scattered]; Oort cloud
│   │                        [statistical, modeled]; damocloid)
│   ├── Structures          (zodiacal cloud; rings [Saturn with real Cassini division structure,
│   │                        Jupiter, Uranus, Neptune]; Lagrange clouds; meteor streams)
│   └── Artificial          (spacecraft, active and historical; [MAY] — data from JPL, ~50 objects)
├── Extragalactic
│   ├── Galaxies            (spiral [Sa–Sd, barred]; lenticular; elliptical [E0–E7]; irregular;
│   │                        dwarf; peculiar/interacting; ring; polar-ring)
│   ├── ActiveGalaxies      (AGN; quasar; blazar [FSRQ, BL Lac]; Seyfert [1, 2]; radio galaxy
│   │                        [FR-I, FR-II]; LINER; starburst)
│   ├── Groups/Clusters     (Local Group; group; cluster [Abell, Coma, Virgo, Fornax]; supercluster;
│   │                        filament; void; Great Attractor; Laniakea)
│   └── Lensing             (strong lens; Einstein ring; arc; multiply-imaged quasar)
├── Transient
│   (supernova [Ia, Ib/c, II-P, II-L, IIn, superluminous, pair-instability]; kilonova; GRB [long, short];
│    FRB; tidal-disruption event; flare [stellar, AGN]; impact event)
├── SurveyLayers            (each all-sky or deep survey is a *layer*, not a type — see §11.3)
└── Anomaly                 (unclassified; contested; speculative — flagged, never hidden)
```

Every object has **exactly one** `taxonomy_id` (its most specific classification) plus a `u64` bitmask of up to 64 additional facets (e.g. "in a globular cluster", "has a relativistic jet", "eclipsing", "in the Messier catalog", "has confirmed exoplanets"). Facets power filtering; the ID powers the legend.

### 11.2 Classification rules

`[MUST]` Deterministic, ordered, and auditable. The ETL's `classify.py` is a rule list where each rule is `(predicate, taxonomy_id, confidence, source_precedence)`, evaluated in priority order, first match wins, with all matches recorded for debugging.

Source precedence (highest first): **specialised review catalog** (e.g. ATNF for pulsars, SNRcat for remnants, Strasbourgh-ESO for PNe) → **SIMBAD's `otype`** (which is itself curated and uses a well-documented otype tree) → **Gaia's `classprob_*` posterior probabilities** → **band-specific survey classification** → **heuristic from photometry/colour**.

Conflicts are recorded, not silently resolved: `classification_history` retains every candidate with its source, and the info panel shows "classified as X (SNRcat); also listed as Y (SIMBAD)". This is honest and it is how real astronomers work.

Render archetype assignment is a **separate many-to-one map** from taxonomy → archetype. Do not conflate them: "pulsar" and "magnetar" are different taxonomy nodes but share an archetype; "elliptical galaxy" and "bulge" are different nodes and different archetypes. The map lives in `docs/taxonomy.md` next to the tree.

### 11.3 The legend

An in-world panel (diegetic, per D10) that is also the primary filter control:

- A row per **visible-in-view** taxonomy class (not all 200 — only those with ≥ 1 object currently in the frustum, which keeps it readable and makes it double as a "what am I looking at" instrument).
- Each row: colour chip + icon glyph, class name, live count, a visibility toggle, and an isolate button.
- Counts update per frame (cheap: they fall out of the selection pass as a histogram written to a GPU buffer, read back with the stats).
- **Survey layers** are a separate section of the same panel: radio (Haslam 408 MHz, HI4PI 21 cm), IR (IRAS 100 µm, 2MASS, WISE, Herschel), optical (DSS2), UV (GALEX), X-ray (ROSAT, Chandra, eROSita), γ (Fermi LAT), CMB (Planck). Each is a toggleable all-sky or tiled overlay rendered in its own false-colour ramp, with the ramp and its units shown in the legend. `[MUST]` label these as false colour with a units scale — showing a Fermi map with a "natural colour" implication would violate D9.
- Colour-code consistency: the legend chip colour, the label colour, the selection ring colour, and the (optional) classification-colouring render mode **all use the same per-class colour**. This is what makes the classification legible across the whole UI. Colours chosen for distinguishability under deuteranopia/protanopia — validate with a colourblind simulator, not by eye.

### 11.4 Colouring modes

A global render-mode switch, always visible in the legend:

| Mode | What drives colour |
|---|---|
| `Natural` (default) | Real photometry → Harre–Heller colour, extinction applied. |
| `Classification` | Taxonomy class colour (the legend palette). |
| `Distance` | Perceptually-uniform ramp over log distance from camera. |
| `Lookback time` | Ramp over light-travel time — makes the "seeing into the past" structure of the galaxy visible. Genuinely novel and cheap. |
| `Radial velocity` | Blue/red Doppler ramp from catalogued RV. Shows Galactic rotation directly. |
| `Proper motion` | Vector-magnitude ramp + optional streak rendering. Shows the solar apex. |
| `Extinction A_V` | Ramp over line-of-sight dust. Reveals the dust lanes as a data layer. |
| `Metallicity [Fe/H]` | From GSP-Spec. Shows the Galactic chemical gradient. |
| `Provenance` | Real vs. derived vs. synthesized. The honesty mode. |

Each mode gets a diegetic colour bar with real units. This is a large fraction of the app's scientific value for very little extra rendering code — it is a change of what feeds the colour output, nothing more.

---

## 12. SEARCH, LISTS, SELECTION

### 12.1 Search index

- **Corpus:** all named objects (~10⁵), all catalog designations for the ~10⁶ first-class objects (~4 × 10⁶ strings), plus ~10⁵ star common names (Bayer, Flamsteed, variable designations, HIP/HD/Gliese).
- **Structure:** a compact **trie over a normalised key** (case-folded, diacritic-folded, punctuation-stripped) with prefix-compressed edges, plus a per-entry payload index into the object table, plus a secondary **BK-tree or symmetric-delete index** for typo tolerance (edit distance ≤ 1 for short queries, ≤ 2 for long).
- `[MAY]` `tantivy` if the hand-rolled version proves awkward — but for 4 × 10⁶ short strings a custom trie will be ~10× smaller and ~5× faster than a general inverted index, and search latency here must be sub-millisecond because it runs on every keystroke.
- **Query kinds:** prefix (as-you-type, < 0.5 ms), fuzzy (≤ 5 ms), **cone search** (`ra, dec, radius` → objects, backed by an HEALPix Nside-512 bucket index), **box/facet search** (`type = globular_cluster AND dist < 10 kpc AND [Fe/H] < −1.5`), and **catalog browse** (paged, sorted by any numeric field).
- **Result ranking:** exact-match → prefix-match → alias-match → fuzzy-match, then by apparent brightness, then by catalog significance. `[MUST]` "M31" resolves to Andromeda Galaxy first, not to some obscure catalog entry that happens to prefix-match.
- **Coordinate parsing:** accept `10h45m27.3s +41d23m18s`, `10 45 27.3 +41 23 18`, `161.3637 41.3884`, `l=121.2 b=−21.6`. This is a solved problem and users expect it.

### 12.2 Faceted query builder

A diegetic panel with chained filters: classification (from the taxonomy tree, multi-select), distance range, magnitude range, colour/temperature range, provenance (real/synthesized), survey layer presence, constellation, and free-text over descriptions. Results stream into a list with live counts. Save queries as named views. This is what turns the app from a viewer into an instrument.

### 12.3 Curated "extraordinary objects" lists

`[MUST]` Generated by **real catalog filters** in the ETL, not hand-typed — so they stay correct as data updates. Each list carries its filter definition and provenance, and shows its own generation date. ~35 lists:

**Stars** — The 100 nearest stellar systems · Brightest apparent · Most luminous known · Largest by radius (UY Sct, Stephenson 2-18, VY CMa) · Most massive (R136a1 and the η Car system) · Hottest / Coolest · Nearest white dwarf (Sirius B) · Nearest neutron star · Nearest black hole (Gaia BH3, ~1 126 pc) · Hypervelocity stars (S5-HVS1 and kin) · Carbon stars · Wolf–Rayet catalog · LBVs and η Carinae-class · Longest-period variables (Mira) · Widest visual binaries · Nearest exoplanet host · Naked-eye stars by constellation.

**Systems** — The ~50 nearest exoplanetary systems · Habitable-zone candidates · Hot Jupiters · Circumbinary (Kepler-16 and kin) · Pulsar planets (PSR B1257+12) · White-dwarf planets · Rogue/free-floating candidates · Most planets in one system (Kepler-90, TRAPPIST-1, HR 8799) · Protoplanetary disks with imaged structure.

**Nebulae and ISM** · All 110 Messier objects · The Caldwell 109 · H II regions by size (30 Doradus, NGC 604, Carina) · Planetary nebulae by morphology · SNRs (Crab, Vela, Cassiopeia A, Tycho, SN 1006) · The Gould Belt and Local Bubble · Molecular clouds by mass (Taurus, Orion A, Perseus, W51, Sgr B2) · Infrared-dark clouds · Supershells.

**Clusters** · The 158 Galactic globulars (Harris) · ω Centauri, M13, 47 Tuc, Terzan 5 · Nearest open clusters (Hyades, Pleiades, Praesepe) · Stellar streams (Sagittarius, Pal 5, GD-1).

**Galactic and extragalactic** · The Galactic centre and Sgr A* · The bar and spiral-arm tracers · Satellite galaxies of the Milky Way (Sgr dSph, LMC, SMC) · The Local Group · M87 and its jet · Nearest quasars and blazars · Strong gravitational lenses and Einstein rings · The Great Attractor and Laniakea.

**Transients** · Historical supernovae (1006, 1054, 1572, 1604) · SN 1987A · GW events (GW150914, GW170817 with its kilonova) · Notable GRBs · Repeating FRBs.

Each list is: a title, a one-paragraph explanation of *why* these are extraordinary (sourced), a filter definition, an ordered member list, and a "tour" action that visits them in sequence using §10.3 travel-to. **Tours are the highest-value, lowest-cost feature in the app** — they turn a data set into an experience, and they are just a queue of travel-to commands.

### 12.4 Selection

- **Picking:** GPU ID buffer at 1/4 res (§6.1 pass 13) for geometry; for stars, a compute pass that finds the nearest selected-candidate star within a screen-space radius, biased by apparent brightness. `[MUST]` bias by brightness — otherwise clicking near Sirius selects an 18th-magnitude star 2 px away.
- **Hover:** a 6 px radius query with a 150 ms debounce, showing a compact tooltip (name + class + distance) before a click commits.
- **Multi-select:** shift-click adds; a rectangle drag selects a screen region and returns a count + the top 20 by brightness, with "add all to a working set" for the query builder.
- **Selection ring:** a diegetic ring drawn at the object's angular size with a fixed pixel-width stroke, colour-coded by class, animated with a 1.5 s settle. Never a bounding box — bounding boxes read as "software", rings read as "instrument".

---

## 13. DATA SOURCES — THE ANSWER TO "WHAT DATA SOURCES ARE BEST"

Ranked by value-per-effort. **Tier 1 is the minimum viable dataset.** Full detail with URLs, sizes, formats, and licenses in Appendix C.

### Tier 1 — Foundational (get these first; nothing works without them)

| Source | What for | Why it's the best choice |
|---|---|---|
| **Gaia DR3** (ESA) | Astrometry + photometry for 1.47 B sources; 2.5 M variable-star summaries; 800 k binary/multiple systems; 10.7 M Cepheids/RR Lyrae; 33 M extragalactic sources; GSP-Spec astrophysical parameters (T_eff, log g, [Fe/H], A_V) for ~470 M; BP/RP mean spectra for ~220 M | There is no alternative at this depth. It *is* the map of the Milky Way. Bulk CSV download from the Gaia Archive, or mirrors at ARI Heidelberg, CDS Strasbourg, and NADC China. |
| **HYG v3** (Astronexus) | ~120 k stars with **common names**, Bayer/Flamsteed, HD/HR/HIP cross-IDs, spectral type | Tiny, permissively licensed, and the single best source for making stars *named* rather than numbered. Do this ingest first — it is an afternoon of work and it transforms the feel of the app. |
| **SIMBAD** (CDS) | Names, aliases, basic data, classifications (`otype`), cross-identifications, bibliography, and a **TAP sync service** for bulk queries | The authority for object identity. Its otype tree is also the best starting point for the taxonomy (§11.1). |
| **NASA Exoplanet Archive** | Cumulative table: > 6 000 confirmed planets with orbital elements, transit parameters, host-star properties | The canonical exoplanet source, with a TAP API and permissive NASA licensing. |
| **JPL SBDB + MPCORB** | Orbital elements for ~1.4 M minor planets and ~4 000 comets, with H/G magnitude parameters | Free, authoritative, bulk-downloadable. Take the top 10⁴ by brightness/interest for D4. |
| **3D dust maps** — Edenhofer et al. 2024 (to 1.25 kpc, parsec-scale, Zenodo 10658339) + Dharmawardena et al. 2024 (all-sky to 2.8 kpc) + Lallement/GaiaMaps3D | Extinction and volumetric dust density | These two together are the reason the Galactic plane will look like the Galactic plane. Non-negotiable for D9. |
| **Planetary albedo maps** — USGS/PDS, NASA Solar System Treks | Surface textures for planets and major moons | Public domain, complete, and the only thing standing between "shaded grey spheres" and "Saturn". |

### Tier 2 — Deep sky (this is what makes it a *galaxy* and not a star field)

| Source | What for |
|---|---|
| **Messier** (110) + **NGC/IC** (Revised NGC/IC, ~13 000) + **Caldwell** (109) | The core DSO backbone. |
| **Sharpless** (313 H II) · **Barnard** (366 dark) · **Lynds** (1 794 dark + 913 bright) · **van den Bergh** (reflection) · **RCW** · **LBN** | The nebula catalogs. Together ~5 000 objects — the population that makes volumetrics worth building. |
| **Strasbourg-ESO PN catalogue** (~1 570) | Planetary nebulae, with morphology. |
| **SNRcat / Ferrand & Bardwadhi catalogue** (~294 Galactic) | Supernova remnants with diameters, ages, and morphology. |
| **Harris (2010 rev.) globular cluster catalogue** (~158) | Positions, metallicities, concentrations, core/half-light radii, velocities. |
| **Cantat-Gaudin et al. open cluster catalogues** (~2 000, Gaia-based, with member lists) | Open clusters *with real membership* — this is what lets a cluster resolve into its actual stars. |
| **Solomon et al. 1987 molecular cloud catalogue** + Ellsworth-Bowers | GMCs with masses, sizes, velocities. |
| **MWPOT18** (galpy) or **McMillan 2017** Galactic potential | Galactic structure and stellar orbit integration. |
| **BeSSeL** (Reid et al., VLBA maser parallaxes) + Vallée/Hou-Han spiral-arm models | Spiral-arm geometry with *measured* distances. |
| **HI4PI** (21 cm all-sky) · **Haslam 408 MHz** · **Planck 857 GHz dust** · **Fermi LAT** · **ROSAT** | All-sky survey layers for §11.3. |
| **DSS2** (STScI) · **2MASS** · **WISE/AllWISE** | Real imagery tiles for far-LOD sprites and survey overlays. **Deliver as HiPS** — the hierarchical progressive-survey format is exactly a tile pyramid and is the correct answer here; do not invent a tiling scheme. |

### Tier 3 — Enrichment (identity, description, and the extraordinary)

| Source | What for |
|---|---|
| **IAU WGSBN / MPC naming citations** | Real provenance for object names. Lovely detail for info panels. |
| **ATNF Pulsar Catalogue** | ~3 400 pulsars with periods, period derivatives, DM, and beam geometry. |
| **Open Supernova Catalog** (~2 000, with light curves) · **Open GRB Catalog** · **FRBCAT / TNS FRB list** | Time-domain transients (§9.4). |
| **GWTC / Gravitational Wave Open Science Center** | GW events with sky localisation and masses. |
| **AAVSO VSX** (~800 k variables) + **GCVS** | Light curves for variable stars. |
| **LEDA / HyperLeda** · **NASA-Sloan Atlas** · **2MRS** | Galaxy morphology (Sérsic n, position angle, inclination, axis ratio), distances, redshifts. |
| **McConnach Local Group catalog** | Satellite galaxies with real structural parameters. |
| **Cosmicflows-4** | Distances and peculiar velocities for ~40 000 galaxies. |
| **CNEOS / Sentry** | Impact risk — a genuinely gripping info-panel element for NEOs. |
| **Harre & Heller (2021) digital colour codes of stars** | The magnitude → display-colour table (§7.1). Small, published, correct. |
| **MIST / PARSEC isochrones** | Mass–radius–luminosity–temperature relations for filling gaps. |
| **Pickles (1998) / Calspec spectral libraries** + **SVO Filter Profiles** | Real SEDs and filter curves — needed to make survey-layer colours meaningful. |
| **WDS** (Washington Double Star, ~147 k pairs) | Multiplicity. |
| **Bailer-Jones `distance` table** (published alongside each Gaia DR) | Bayesian distances. Use instead of `1/parallax`. `[MUST]` |

### 13.1 Licensing summary

Personal use (D1) means licensing is a **credits obligation, not a legal blocker**. Still:

| Data | License | Obligation |
|---|---|---|
| Gaia DR3/DR4 | **CC BY-NC 3.0 IGO** | Attribute ESA/Gaia/DPAC with the release-specific credit line and DOI. Non-commercial — fine here. |
| SIMBAD, VizieR, CDS services | CDS general terms; most catalogs individually licensed | Attribute CDS and the individual catalog references. |
| NASA/IPAC (Exoplanet Archive, 2MASS, WISE, IRSA) | Generally open with attribution | Attribute Caltech/IPAC/NASA. |
| JPL Horizons, SBDB, NAIF | US Government work | Attribute JPL/Caltech. |
| DSS (STScI) | Attribution required, plus original survey credits (POSS, UK Schmidt, AAO) | Credit the photographic surveys. |
| PDS / USGS planetary maps | Public domain (US Gov) | None, but credit is good practice. |
| Zenodo-hosted academic maps (Edenhofer, etc.) | Usually CC BY 4.0 | Attribute the paper. |
| HYG | CC Attribution / public-domain blend | Attribute Astronexus. |
| MIST/PARSEC | Free for non-commercial research | Attribute. |

`[MUST]` `etl/lineage.py` auto-generates `data/CREDITS.md` from the actual sources used in a build, with versions, access dates, DOIs, and license text. Generate it, do not hand-maintain it — hand-maintained credit files rot.

### 13.2 The DR4 plan (D7)

- **Now:** build the adapter layer against DR3. Prove the whole pipeline end-to-end on one sky region (Spike P0-S5).
- **Before 2 Dec 2026:** read the published DR4 draft data model; write the DR4 `ColumnMapping` as far as possible; run the adapter against the **DR4 pre-released astrometric time-series** data to smoke-test the plumbing.
- **2 Dec 2026 – mid Jan 2027:** ingest DR4 in full, re-run the octree build, re-validate (§17), re-calibrate the synthesis model. Budget 2 weeks.
- **Design rule:** `[MUST]` no downstream stage may reference a Gaia column name. Only the adapter touches source column names. If that rule holds, DR4 is a config change plus a re-run.

---

## 14. DISPLAY, RENDER, AND SIMULATION SETTINGS

All persisted to `%APPDATA%/galaxia/config.toml` (TOML, human-editable, schema-versioned, with a `--reset-config` flag). Grouped as the brief requests:

### 14.1 Display
Output resolution & window mode · internal render scale (50–100 %) · upscaler (Off / FSR2 Quality / FSR2 Balanced / FSR2 Performance / XeSS) · TAA (off/low/high) · HDR output (if the display reports it; otherwise tone-map to SDR) · brightness, exposure bias, white point · colorblind-safe palette toggle · VSync / uncapped / frame-rate cap · UI scale · label size · high-contrast UI mode · screenshot resolution multiplier (supersample up to 4×, independent of render scale).

### 14.2 Render
Quality preset (Low / Medium / High / Ultra / Custom) driving everything below · **max visible stars** (the governor's ceiling, 1 M – 100 M) · octree draw distance θ (30°–120°) · star rendering method (software raster / billboard — exposed because it's a genuine quality/perf trade and worth having for debugging) · glare method (screen-space / perspective-correct) and glare strength · resolved-star detail level · volumetric on/off, resolution (1/2–1/16), max steps (16–256), temporal rate (1/1–1/32), empty-space skipping · volumetric light shafts · accretion-disk geodesic raymarching on/off and step count · shadow quality for volumetric scattering · texture cache size · mesh LOD bias · transparency overdraw cap · DoF, chromatic aberration, grain, vignette, diffraction spikes (all off by default per D9) · **colouring mode** (§11.4) · **visibility mode** (`Observed` / `Enhanced`, §7.7) with per-component gain sliders · grid overlays (equatorial, galactic, supergalactic, ecliptic, HEALPix, alt-az) · constellation lines, figures, and boundaries · label density and label budget · survey layers with individual opacity and colour ramp.

### 14.3 Simulation
Current epoch and time rate (with a numeric entry field and the discrete-jump buttons from §9.1) · light-time correction (`Apparent` / `True` / `Astro J2000`) · aberration on/off · precession/nutation on/off · proper-motion animation on/off · ephemeris accuracy (fast truncated / full series) · small-body count (100 / 1 000 / 10 000) · comet activity model on/off · variable-star animation on/off · transient-event display on/off · Galactic-orbit integration on/off and integrator step · procedural synthesis on/off, model version, and **seed** · synthesis-vs-real blend seam visibility (debug) · unit system (metric / astronomical / imperial · pc / ly · magnitudes Vega / AB).

### 14.4 Data & system
Installed tiers and per-dataset status · data directory · streaming thread count and I/O priority · cache sizes · governor enable and aggressiveness · **live performance overlay** (GPU pass timings, visible stars, resident nodes, VRAM, streaming MB/s, governor state) — `[MUST]` this exists from week 1, it is the primary development instrument · telemetry: none (D1) · crash dumps: local only.

### 14.5 First-run
A short diegetic "cockpit initialisation" sequence that: detects the GPU and VRAM, picks a preset, verifies the installed data tier, offers a guided tour (3 stops: the Solar System, the Orion complex, the Galactic centre), and shows the legend and the search hotkey. Under 3 minutes, skippable, and re-runnable.

---

## 15. VISUAL STYLE GUIDE (D9: science-faithful)

Written down because "beautiful" is otherwise unbuildable.

1. **Black is black.** No ambient fill, no sky fog, no blue-tinted shadows. Space is dark. The only ambient term is the physically-motivated interstellar radiation field and the CMB, both negligible in the visual band.
2. **Dynamic range is the aesthetic.** A single frame may contain a 10⁶ L☉ O star and a 15th-magnitude background field star. Auto-exposure with a key-value model and temporal adaptation (Pattanaik et al. 2000) plus filmic tone mapping (Duiker) is what makes this look photographic rather than washed out.
3. **Colour comes from data.** Never from a palette. Star colours from Harre–Heller; nebula colours from real line ratios; galaxy colours from real broadband photometry; survey layers from their published ramps.
4. **Glare is physical.** The `c/θ²` veiling-luminance pyramid (§7.4), not a Gaussian bloom. Bright stars should have that specific soft, wide, slightly-structured halo that real optics produce.
5. **Extinction is visible.** The Galactic plane should look *obscured*, with dark lanes cutting through star fields, because that is what 3D dust maps say. If the plane looks uniformly bright, the extinction is wrong.
6. **Nothing is invented.** No decorative nebulosity, no randomly-placed stars, no artistic colour grading in `Observed` mode. If it renders, it corresponds to data or to a calibrated model, and the provenance badge can say which.
7. **Motion is smooth or absent.** Jerk-limited camera, dithered cross-fades on every LOD transition, no popping, no snapping. The one exception: transient events (supernovae, GRBs) *should* be abrupt, because they are.
8. **UI is instrument, not overlay.** Diegetic panels look like they belong to an observation platform: thin strokes, monospace numerics, real units, no drop shadows, no rounded-corner cards. Reference feel: a mission-control display, not a game HUD.
9. **Silence in the render.** No lens flares by default, no chromatic aberration, no grain. All are available as off-by-default toggles for users who want the cinematic look — but the default frame must be one a planetary scientist would not wince at.
10. **Empty is allowed.** A high-latitude, dust-free, arm-interstitial field should look *sparse*. That is the truth, and after the density of the Galactic centre it is also dramatic. Do not fill it.

---

## 16. BUILD PLAN — PROTOTYPE FIRST, THEN ITERATE

### 16.1 Build philosophy (product-owner directive, 2026-10-23)

> **"Prioritise a working prototype and iterate on it."**

This overrides the earlier spike-by-spike sequencing. The consequences are structural:

1. **Walking skeleton first.** By the end of **day 2** the app must run and display *something* recognisable as a starfield that the camera can fly through. It will be a hardcoded array of ~10 000 bright stars drawn as points — fake, crude, and irrelevant. Its only job is to make every subsequent change a *modification to a running app* rather than an *assembly of components that meet for the first time in week 8*. Integration risk is the dominant schedule risk in graphics projects; a walking skeleton eliminates it.
2. **Every iteration ends runnable, measurable, and recorded.** No iteration may end in a state where the app does not build and run. No iteration may exceed **6 working days** — if it looks like it will, cut its scope, do not extend it.
3. **Beauty before rigour, but rigour before scale.** The order is deliberately: get something on screen → make it beautiful → make it correct → make it big → make it interactive. The one hard exception below.
4. **`[MUST]` The precision architecture (§H1) must land no later than Iteration 4.** It is the only piece that cannot be retrofitted cheaply, because every transform in the codebase depends on it. The skeleton and iterations 1–3 are permitted to live in a single f64-relative-to-Sun frame that works fine out to ~1 kpc — that covers Orion completely — and Iteration 4 replaces it with the full frame hierarchy before any galactic-scale work begins. **Do not let the skeleton's shortcut become permanent.** This is the single highest-risk item in the schedule and it is called out here rather than buried in a table.
5. **Depth before breadth.** Each iteration deepens one capability across the whole pipeline. Never start a second capability before the current one is demonstrated.

### 16.2 The prototype region — ORION (confirmed)

| Parameter | Value |
|---|---|
| Cone centre | **α = 83.82°, δ = −5.39°** (ICRS; ≈ 05h 35m 17s, −05° 23′ — just north of Alnilam) |
| Radius | **20°** |
| Magnitude limit | `phot_g_mean_mag < 20.0` |
| Expected rows | ~35–50 M |
| Raw CSV | ~14–20 GB |
| Baked artifact (T0) | **~1.1–1.6 GB** |
| Gaia epoch | 2016.0 |

**Why this cone is an excellent prototype region** — it is unusually well-stocked with hard cases:

- **The Orion Molecular Cloud Complex**: M42/M43 (Orion Nebula), the Trapezium and θ¹ Ori C (the illuminating O6 star — the light source for volumetric god rays), M78, NGC 1977 (Running Man), Barnard's Loop, the Flame Nebula, IC 434 (Horsehead), LDN 1630 and the Orion A/B clouds. Every nebula archetype in §11.1 is represented within a few degrees.
- **Named bright stars**: Betelgeuse (M2 Iab, resolved disc + real variability), Rigel (B8 Ia), Bellatrix, Alnilam/Alnitak/Mintaka, Saiph, plus Sirius and Procyon at the cone edge. Exercises the full photometric chain including Harre–Heller colours across O/B/A/K/M.
- **Open clusters with real member lists**: NGC 1981, the Pleiades and Hyades near the cone edge, and dozens of smaller ones from Cantat-Gaudin. Tests cluster LOD resolve-in.
- **Extreme density contrast**: the Trapezium core versus the empty 15° gap toward high galactic latitude. This is exactly the dynamic range the governor and the LOD criterion must handle, and it is why Orion is a *better* prototype than a uniformly dense field.
- **3D dust coverage**: the entire region sits inside the Edenhofer et al. 2024 1.25 kpc map, so extinction and volumetric density are real throughout, with no fallback-to-model seam to confuse the results.

`[MUST]` Keep a **tiny fallback region** — the Pleiades alone, 2° radius, ~1 M stars, ~400 MB CSV — checked in as a test fixture. If the big query fails, throttles, or corrupts, Iteration 1 proceeds unblocked. Never let a download be a schedule dependency twice.

### 16.3 THE BUILD LOG — mandatory from the first commit

**Purpose:** this project will be built across many sessions, possibly months apart, by an engineer (human or AI) who does not remember what happened last time. The build log is the mechanism that makes resumption cheap. `[MUST]` It is not optional documentation; it is part of the deliverable of every work session, and **a session that does not update it did not finish.**

Three files, each with a distinct job:

| File | Job | Mutability |
|---|---|---|
| `BUILDLOG.md` | Chronological narrative. The story of the build. | **Append-only.** Never edit or delete an entry. Corrections are new entries. |
| `docs/build/STATUS.yaml` | Machine-readable current state: what works, what's next, what's blocked, latest numbers. | **Rewritten** at the end of every session. Single source of truth for "where am I". |
| `docs/build/measurements.csv` | Every recorded number, one row each, so trends are plottable. | Append-only. |

#### 16.3.1 `BUILDLOG.md` format

```markdown
# Build Log

## Session 014 — 2026-12-08 — Iteration 3 (software rasteriser)

**Goal:** replace point-sprite stars with the compute software rasteriser; hit ≤5.5 ms
for 10M stars at 1440p on the Titan RTX.

**Started from:** commit `a1b2c3d`. Previous session ended with 8M billboard stars at
11.2 ms — over budget, which is why this iteration exists.

**Done:**
- Implemented tile-binning pass over the compacted visible list (8×8 tiles).
- Software rasteriser with shared-memory tile accumulation, resolved once per tile.
  Deliberately chose shared-memory over per-fragment global atomics — Turing's 6 MB
  L2 makes global atomics the bottleneck (see PLAN §1.1.2).
- Added perspective-correct solid-angle luminance (the 2025 paper's central point;
  without it, wide-FOV views are measurably wrong at the screen edges).

**Measured:**
| metric | before | after | budget |
|---|---|---|---|
| 10M stars @1440p | 11.2 ms | **4.1 ms** | 5.5 ms ✔ |
| 20M stars @1440p | n/a | 7.9 ms | — |
| visible-star ceiling at 60fps (volumetrics off) | 6M | **17M** | 8–10M ✔ |
| peak VRAM | 1.9 GB | 2.1 GB | 22 GB ✔ |

**Blockers hit:**
- `VK_EXT_shader_atomic_float` is NOT exposed by wgpu on this driver (546.33).
  Resolved by the u32 fixed-point accumulation path, which we were building anyway
  per PLAN §1.1.2. Cost: one extra normalisation pass, ~0.2 ms. → **ADR-0007**.
- Tile binning produced non-deterministic ordering → shimmering under temporal
  accumulation. Fixed by sorting within tile by node ID. Determinism test now passes.

**Not done / discovered work:**
- Glare pyramid is still the naive Gaussian. It reads as "bloom", not "veiling
  luminance". Needs the bicubic mip-weighted version (PLAN §7.4). → Iteration 3b.

**Screenshot:** `docs/build/shots/2026-12-08-iter3-orion.png`

**Next (precise):** implement the bicubic glare pyramid from PLAN §7.4, screen-space
variant only. Verify against the 20 Harre–Heller reference stars (ΔE₂₀₀₀ < 3).

**Commit:** `e4f5a6b`
```

`[MUST]` Rules for entries:

- **Numbers, not adjectives.** "Faster" is worthless; "11.2 ms → 4.1 ms against a 5.5 ms budget" is a record. Every performance claim is a measurement row.
- **Record failures and dead ends.** They are the most valuable content in the log — they stop the next session repeating them. A session that discovered something doesn't work was not a wasted session, *provided it wrote it down*.
- **Every entry ends with a precise `Next`.** Not "continue work on volumetrics" — the specific next action, specific enough that a cold reader can start without re-deriving context. This field is the single most important line in the entry.
- **Link commits and ADRs.** The log is the index into the repo's history, not a replacement for it.
- **Never rewrite history.** Append-only. If an entry was wrong, write a new entry saying so.
- **Screenshot every iteration.** `docs/build/shots/YYYY-MM-DD-iterN-<what>.png`. The visual record is what makes the project feel real over months, and it is how you notice regressions that numbers miss.

#### 16.3.2 `docs/build/STATUS.yaml` — the resume file

```yaml
schema: 1
updated: 2026-12-08T21:40:00+08:00
session: 14
commit: e4f5a6b
iteration: 3
iteration_name: "software rasteriser + HDR"
branch: main

phase: prototype            # prototype | M1 | M2 | ... | done
skeleton_running: true
last_green_ci: e4f5a6b

working:                    # demonstrably works, verified this session or later
  - "wgpu window + frame graph (20 passes stubbed, 6 live)"
  - "octree loader + async streaming + LRU resident set (Orion T0)"
  - "GPU-driven star selection into indirect buffers"
  - "software rasteriser, shared-memory tile accumulation, perspective-correct solid angle"
  - "camera rig: exponential speed, critically-damped springs, jerk limit, focus modes"
  - "floating-origin rebasing (SINGLE-FRAME shortcut — see blockers)"

not_working:                # known broken, do not trust
  - "glare is a naive Gaussian, not the bicubic c/theta^2 pyramid"
  - "auto-exposure strobes during fast travel (>1 kpc/s)"

next_action: |
  Implement the bicubic mip-weighted glare pyramid from PLAN §7.4, screen-space variant
  only. Then verify 20 reference stars against Harre-Heller published colours, ΔE2000 < 3.

next_iteration: 4
next_iteration_scope: |
  Precision architecture: frame hierarchy, split-double view transform, reversed-Z,
  derived near plane, render-frame scale exponent. Retire the single-frame shortcut.
  MUST land the CI torture test (<0.1 px drift, 100 kpc -> 1e4 km).

blocked: []

measurements_latest:
  stars_visible_ceiling_60fps: 17_000_000
  star_raster_10M_1440p_ms: 4.1
  star_raster_20M_1440p_ms: 7.9
  frame_ms_orion_wide: 9.8
  peak_vram_gb: 2.1
  resident_leaves: 412
  stream_mb_s: 180
  dataset: "T0-orion (Gaia DR3, 41.2M stars, 1.34 GB)"

environment:
  gpu: "NVIDIA TITAN RTX 24GB"
  driver: "546.33"
  wgpu: "30.0.1"
  rust: "1.83.0 stable"
  os: "Windows 11 24H2"
  free_nvme_gb: 168

adrs: ["0001-frame-hierarchy", "0002-software-rasteriser", "0003-egui-for-prototype-ui",
       "0004-orion-as-prototype-region", "0005-fsr2-default-upscaler",
       "0006-streaming-partitioned-etl", "0007-u32-fixed-point-star-accumulation"]
```

`[MUST]` Rewrite this file at the end of every session, even a session that achieved nothing. A stale STATUS.yaml is worse than none, because it is trusted.

#### 16.3.3 The resume protocol

Print this into `BUILDLOG.md`'s header and into `CONTRIBUTING.md` so it is the first thing any session sees:

```
RESUMING WORK — do these in order, do not skip any:

 1. Read docs/build/STATUS.yaml.                    → where the project is
 2. Read the last THREE sessions in BUILDLOG.md.    → what was just happening
 3. Read docs/adr/INDEX.md and skim any ADR touched
    in those three sessions.                        → why it is that way
 4. git log --oneline -20  and  git status.         → is the tree clean?
 5. cargo build && cargo test && cargo run -- --scene orion_wide
    Confirm the app runs and looks like the last screenshot in
    docs/build/shots/.                              → TRUST NOTHING UNVERIFIED
 6. Run the benchmark scenes: cargo run -- --bench
    Compare against measurements_latest in STATUS.yaml.
    If a number moved >10%, find out why BEFORE adding features.
 7. Only now: start on `next_action`.

If step 5 or 6 fails, the session's goal is to fix that — not to build new things
on top of a broken base. Update STATUS.yaml to say so.
```

`[MUST]` **Trust nothing that is not verified by running it.** The log is a map, not the territory. Sessions routinely end with an optimistic `working:` list; step 5 exists because of that.

#### 16.3.4 `docs/build/measurements.csv`

```
timestamp,session,iteration,commit,scene,stars_visible,star_raster_ms,volumetrics_ms,glare_ms,post_ms,frame_ms_total,fps,peak_vram_gb,resident_leaves,stream_mb_s,notes
2026-12-08T21:31:00+08:00,14,3,e4f5a6b,orion_wide,10000000,4.1,0.0,1.4,0.9,9.8,102,2.1,412,180,"shared-mem tile accumulation"
2026-12-08T21:31:04+08:00,14,3,e4f5a6b,orion_trapezium,4200000,2.2,0.0,1.4,0.9,6.1,163,2.0,88,"dense core, tile saturation active"
```

One row per (session × benchmark scene). Plot `frame_ms_total` over time weekly — a slow creep of +0.3 ms/week is invisible day-to-day and fatal by month three.

### 16.4 The iteration ladder

Each row is one iteration: 3–6 working days, ending runnable and recorded.

| It. | Days | Deliverable — *the app must do this at the end* | Key work | Gate (measured) |
|---|---|---|---|---|
| **0** | 1–2 | **Walking skeleton.** A window opens. 10 000 hardcoded bright stars render as points. Camera flies with exponential speed and damped rotation. Debug overlay shows FPS. | winit + wgpu device + swapchain; frame-graph skeleton; WGSL **hot-reload**; GPU timing query sets; input→action mapping; `BUILDLOG.md` + `STATUS.yaml` created. | App runs at 60 fps. Overlay shows per-pass timings. **Log entry 001 written.** |
| **1** | 4–6 | **Real data.** The Orion cone renders ~35 M real Gaia stars with correct positions and crude colours. Streaming works — you can fly into the region and it refines. | Python ETL slice (T0): TAP query → normalise → astrom → quantise → Morton → octree → leaf pack. Rust: octree loader, async streaming, LRU resident set, GPU node cull, billboard/point stars. HYG named stars. | ≥ 5 M stars visible; streaming keeps up at 50 pc/s; astrometry < 1 arcsec vs Stellarium for 100 named stars. |
| **2** | 4–6 | **Beautiful.** Stars are physically coloured, glare is the perceptual `c/θ²` pyramid, HDR + auto-exposure + filmic tonemap, FSR2 → 4K. This is the first screenshot worth showing anyone. | Software rasteriser (shared-memory tile accumulation, perspective-correct solid angle); Harre–Heller colour chain; glare pyramid with bicubic reconstruction; exposure adaptation; FSR2. | **10 M stars @1440p ≤ 5.5 ms**; 20 reference stars within ΔE₂₀₀₀ < 3; FSR2 ≤ 1.2 ms; 60 fps in `orion_wide`. |
| **3** | 3–4 | **Correct at scale.** Continuous zoom from 200 pc out to 100 kpc and in to 10⁴ km with no jitter, no popping, no discontinuity. | The full §H1 precision architecture: frame hierarchy, floating origin + hysteresis, split-double view transform, reversed-Z, derived near plane, render-frame scale exponent. **Retire the skeleton's single-frame shortcut.** CI torture test. | **< 0.1 px centroid drift** on a fixed star across the full zoom range; zero visible discontinuity at frame swaps. `[MUST]` |
| **4** | 5–6 | **Volumetric.** The Orion Nebula has real dust, real emission, and god rays from θ¹ Ori C. Flying *into* the cloud works and does not white out. | Froxel grid; hybrid density field (Edenhofer 3D dust + per-object analytic volumes from Sharpless/Lynds parameters + detail octaves); single scatter + 6-step shadow cone march; SH ambient; temporal reprojection at 1/8 rate; line-emission palette (Hα/[O III]/[S II]); per-froxel optical-depth saturation. | ≤ 2.2 ms at ¼ of 1440p; stable during a slow orbit; recognisable against a real reference image of M42; no white-out inside LDN 1630. |
| **5** | 4–6 | **Interactive.** Click a star or nebula → an info panel with real data. Jump to any of ~200 hardcoded objects. Time dial moves planets; light-time toggle visibly shifts outer planets. | GPU ID picking; selection + hover; egui info panel (**not** diegetic — deliberate prototype shortcut, saves 2–3 weeks); object-jump list; time system with sub-stepping; VSOP87/ELP ephemeris; light-time τ solve with the three-way toggle. | Picking works on stars and on volumetric objects; ephemeris < 1 arcmin vs Horizons; τ toggle produces a visible shift for Jupiter. |
| **6** | 3–4 | **Governed and honest.** Frame rate holds 60 fps under adversarial camera work. Provenance is visible. Settings persist. | Adaptive quality governor (PI controller on frame time, §6.6) wired to real readback; preset table finalised **from measurement**; provenance badges; settings TOML persistence; golden-image harness; 20 benchmark scenes. | 60 fps sustained for 5 minutes of continuous zoom Galactic-plane ↔ planetary; VRAM < 16 GB; no regressions > 10 % vs baseline. |

**Total: 24–32 working days ≈ 5–6.5 weeks full-time.** This is faster than the 8–9 week estimate in v1.1 of this plan for two reasons: the walking skeleton removes integration risk (components never meet for the first time in week 8), and the Orion-only scope removes all the generalisation work that a multi-region prototype would have demanded.

At 20 h/week part-time: **~12–14 weeks.** With a strong AI pair on the ETL, octree builder, config/serde, and test fixtures: **~4–5 weeks full-time equivalent** — the AI leverage concentrates in Iteration 1 and 5, not in the shader-heavy 2 and 4.

**Two intentional deviations from the rest of this plan, both to be undone after the prototype:**
- The UI is **egui, not diegetic** (D10). Recorded as ADR-0003. The diegetic system is M6 work.
- Data is **Orion-only, T0**. All other tiers are deferred until the prototype is in daily use.

### 16.5 After the prototype — iteration waves

M1–M8 from the earlier revision are **not** discarded; they become the shape of post-prototype work, executed as 2–3 week iterations with the same discipline (runnable at the end, measured, logged, ADR'd for every deviation).

| Wave | ~Weeks | Content | Prerequisite |
|---|---|---|---|
| **W1 Data at scale** | 6 | T1 Core all-sky build; streaming-partitioned ETL at scale (§8.4.1); full taxonomy; identity resolution/cross-matching; search index; `data_caps()`. | Prototype in daily use. T1 build validated against the 60 GB peak-disk budget. |
| **W2 Objects** | 8 | Every archetype in §11.1; imagery tiles + HiPS + virtual texturing; **the diegetic UI: GPU SDF text, decluttered labels, radial menus, in-world panels** (retires ADR-0003); interactive legend; colouring modes. | W1. |
| **W3 Simulation** | 4 | Small bodies, comets with two tails, exoplanet systems, variable stars, pulsar beaming, transients (SN/GRB/FRB/GW), Galactic orbits, proper-motion animation, the Andromeda encounter. | W2 for the info-panel surface. |
| **W4 Volumetrics & phenomena** | 6 | All four 3D dust datasets; superbubbles and the Local Bubble; zodiacal light; Galactic cirrus; synchrotron with Doppler beaming; accretion-disk geodesics; the Enhanced-visibility mode with its badge. | Iteration 4's foundation. |
| **W5 Procedural synthesis** | 4 | The Besançon-style model, deterministic per-node seeding, and the calibration/validation campaign against real Gaia counts (§8.7). | W1 — needs the overlap region to calibrate against. |
| **W6 UX & polish** | 5 | Faceted query builder; all 35 curated lists + tours; first-run sequence; accessibility; label level-of-abstraction across all zoom regimes. | W2. |
| **W7 Perf & hardening** | 4 | Full profiling campaign; VRAM audit; 24 h warp soak; determinism verification; fuzzing; golden-image regression suite. | Everything. |
| **W8 Data lifecycle** | 3 | **The Gaia DR4 migration** (timed for January 2027); T2 Standard build; content manager; CREDITS regeneration. | W1. Adapter layer must already exist. |

**~40 weeks post-prototype.** Combined with the 5–6.5 week prototype: **~46 weeks full-time**, or ~18–24 months at a sustainable part-time pace.

### 16.6 "Always working" rules

`[MUST]` These are the operational rules that make iterate-on-a-prototype safe:

1. **`main` is always runnable.** No long-lived feature branches. Small commits, each leaving the app working.
2. **CI runs on every commit:** `cargo build`, `cargo clippy -D warnings`, `cargo test`, the precision torture test, the determinism test, and the golden-image diff. A red `main` stops all other work.
3. **Every session ends with:** `cargo test` green → screenshot → `measurements.csv` row → `STATUS.yaml` rewritten → `BUILDLOG.md` entry appended → commit. **In that order. The commit is last so the log entry's commit hash is real.**
4. **No feature is "done" until it has a number attached.** A rendering feature is done when its GPU cost is measured and recorded. A data feature is done when it validates against an authoritative source.
5. **Deviations get an ADR, immediately, one page.** Context / decision / consequences. Numbered sequentially, never deleted, indexed in `docs/adr/INDEX.md`.
6. **When the plan and reality disagree, reality wins — but write it down.** The plan is a map produced before contact with the terrain. The build log is the record of the terrain.
7. **If an iteration overruns its 6 days, cut its scope, not the log.** A half-finished iteration recorded honestly is worth more than a finished one recorded vaguely.

### 16.7 Prototype exit review

At the end of Iteration 6, answer in writing (as a BUILDLOG entry and an ADR):

1. What did the star rasteriser *actually* cost on the Titan RTX at 5 M / 10 M / 20 M stars, at 1440p and at native 4K? How far was the §1.1.1 extrapolation from reality?
2. Which §6.2 preset becomes the default? Can volumetrics and 10 M stars coexist inside 13 ms at 1440p internal?
3. Did the precision architecture survive the torture test, and was the skeleton's single-frame shortcut retired cleanly?
4. How long did the T0 ETL take, what did it cost in peak disk, and does the streaming-partitioned design hold at T1 scale?
5. Is the Orion region *beautiful*? Look at the screenshot next to a real ESO/Hubble image of M42. If it is not close, W4 needs more time than budgeted — find that out now.
6. **Revised target statement** to replace §1.1.1, written from measurement rather than extrapolation.

Then, and only then, start W1.

---

## 17. TESTING, VALIDATION, AND PERFORMANCE VERIFICATION

### 17.1 Astrometric correctness (the app is a scientific instrument; it must be right)

| Test | Reference | Tolerance |
|---|---|---|
| Planetary positions, 10 bodies × 20 epochs spanning ±500 yr | JPL Horizons (queried offline, results baked into fixtures) | < 1 arcmin |
| Lunar position, 100 epochs | Horizons | < 10 arcsec |
| Named-star positions, 500 stars | Gaia DR3 catalog values + Stellarium cross-check | < 0.1 arcsec at J2000; < 1 arcsec at ±100 yr (proper motion) |
| Coordinate transforms, 10⁶ random sky points | `astropy.coordinates` offline; round-trip property test | < 1 × 10⁻¹² rad round-trip |
| Time-scale conversions | IERS/SOFA reference values | < 1 ms over 1950–2100 |
| Extinction, 100 lines of sight | Published A_V values from the dust-map papers | < 0.15 mag |
| Star colours, 20 spectral standards | Harre & Heller (2021) published values | ΔE₂₀₀₀ < 3 |
| Absolute magnitude, 1 000 stars with good parallaxes | Recompute from Gaia; compare to literature values | < 0.05 mag |
| Exoplanet orbital periods, all 6 000 | NASA Exoplanet Archive | exact (data round-trip) |
| Ephemeris vs. a Kepler solver | Independent implementation | < 1 × 10⁻⁸ rad |

`[MUST]` These live in `tests/astrometric/` as data-driven tests with fixtures committed to the repo (so they run offline in CI) and a `--refresh-fixtures` flag that re-queries Horizons/astroquery.

### 17.2 Statistical validation of the synthesis model

Per §8.7: compare synthesized vs. real Gaia star counts in `(l, b, m)` cells over the overlap region. Acceptance: counts within 10 %, mean colour within 0.2 mag, over ≥ 10⁴ cells including plane/arm/inter-arm/high-latitude strata. Produce a validation report with plots as an ETL artifact — this is the evidence that the hybrid approach is honest.

### 17.3 Performance regression

- ~20 named benchmark scenes in `tests/scenes/`: "Galactic centre at 8 kpc", "Inside the Orion Nebula", "Solar System from Saturn", "Warp to M31", "Full-sky at 200 pc", "M13 core", "Sgr A* accretion disk", etc.
- Each records per-pass GPU timings, CPU frame time, visible star count, resident node count, and peak VRAM to a CSV.
- A runner script diffs against the committed baseline and **fails on > 10 % regression** in any pass. Run weekly and before every milestone exit.
- `[MUST]` use real GPU timestamps (`wgpu` query sets), never CPU wall-clock around a submit — that measures nothing.

### 17.4 Visual regression

Golden images per benchmark scene, compared with a perceptual metric (SSIM or a CIEDE2000 mean) rather than bitwise. Tolerance tuned so that anti-aliasing jitter and temporal-reprojection noise pass but real regressions fail. Commit the goldens; regenerate deliberately with a `--bless` flag.

### 17.5 Robustness

- **Precision torture test** (§H1) runs in CI on every commit.
- **Determinism:** fixed seed + fixed input sequence → bit-identical framebuffer over 1 000 frames. Verify in CI. This catches an entire class of race and ordering bugs.
- **Stress:** 24 h continuous random-warp soak; assert monotonic RSS and VRAM.
- **Fuzz:** random camera states, random epochs (including ±10¹⁰ yr), random focus switches, malformed config files, truncated data files, missing tiers. Nothing may panic; all failures must be recoverable with a user-visible message.
- **Validation layers on in debug builds** (`wgpu` Vulkan validation), plus `cargo clippy -D warnings` and `cargo deny` for license/advisory checks.

### 17.6 Profiling discipline

- `puffin` for CPU flamegraphs, in-app from week 1.
- Per-pass GPU timing surfaced in the debug overlay permanently — not behind a flag.
- NSight Graphics / RenderDoc integration: `[MUST]` all resource names set via wgpu labels so captures are readable. An unnamed GPU capture on a 20-pass renderer is undiagnosable.
- `docs/perf_budgets.md` is the source of truth for §6.2 and is updated whenever the budget changes, with the measurement that justified it.

---

## 18. RISK REGISTER

| # | Risk | P | I | Mitigation | Contingency |
|---|---|---|---|---|---|
| R1 | Frame budget not met on the Titan RTX (native 4K with 10 M+ stars + volumetrics + post) | **Certain at native 4K** | High | Already resolved in §1.1.1/§6.2: 1440p internal + FSR2 → 4K, 8–10 M star budget, governor from M1. P0-S2 measures the real numbers in week 3. | Drop internal resolution to 1080p; cap stars at 5 M; tile-based rasteriser with shared-memory accumulation; or accept 30 fps for galactic-plane views. **A second Titan RTX over NVLink does NOT help** — split-frame rendering is not worth building for a personal project. |
| R2 | Gaia DR4 migration invalidates the ETL | **Certain** (it will change) | Medium | D7: adapter layer, no downstream source-column references, rehearse with the DR4 pre-release, 2-week budgeted milestone. | Ship DR3 indefinitely — it is perfectly good data. DR4 is an upgrade, not a requirement. |
| R3 | Precision architecture is subtly wrong; jitter appears late and is expensive to fix | Medium | **Critical** | P0-S1 is spike #1 with a hard numeric criterion and a CI torture test. | None — this must be right. Hence it is first. |
| R4 | Synthesis seam is visible (density/colour discontinuity where real data ends) | Medium | High | §8.7 calibration + validation report; deterministic seeding; dithered blend over a magnitude/distance range rather than a hard boundary; provenance colouring mode to inspect it. | Widen the real-data tier (Standard instead of Core); or reduce synthesis to unresolved volumetric haze only. |
| R5 | wgpu feature gaps on Turing (atomic float, indirect-draw limits, bindless) | Medium | Medium | Bindless **is** available via `Features::BINDLESS` (Vulkan 1.2+ `descriptor_indexing` — Turing supports it). RT is not needed (§2.2). `VK_EXT_shader_atomic_float` must be checked on the real driver in P0-S2; build the fixed-point u32 fallback regardless. | Pin the wgpu version and stay on it; upstream a PR if something is missing. Worst case: raw Vulkan (ash) for the one offending pass — Turing is well supported there. |
| R6 | Streaming starvation during warp travel → visible pop-in or stalls | Medium | Medium | Coarse LOD always resident (§H2); priority queue; never block the render thread; render parent representatives while children load. | Throttle warp speed to the measured stream rate; add an NVMe requirement for Tier 3. |
| R7 | Diegetic UI (D10) costs far more than expected | **High** | Medium | Budget it explicitly in M3/M6 (§10.3). GPU SDF text is a known quantity. egui escape hatch for settings. Label budget of 64 keeps decluttering tractable. | Fall back to a hybrid (2D docked panels for info/settings, 3D labels only) — a documented ADR, not a failure. |
| R8 | ETL scale: 2 TB of Gaia CSV on the build machine | Medium | Medium | DuckDB streaming reads over CSV/Parquet, chunked by `source_id` range; never load the full table. §1.1 host spec. | Restrict to G < 19 (Tier 1) and skip the Full tier. |
| R9 | Science-faithful default looks "empty" or underwhelming to the user | Medium | Medium | §7.7 Enhanced mode with an honest badge; §11.4 colouring modes that reveal hidden structure; tours (§12.3) that take the user to the spectacular things. | Raise the default exposure/gain; make Enhanced the default but keep the badge. |
| R10 | Scope creep — this plan is already ~48 weeks | **High** | High | §2.2 non-goals, checked before any addition. Milestone exit criteria are numeric. Decision log is locked. | Cut in this order: Tier 3 data → extragalactic depth → transients → colouring modes beyond 4 → accretion-disk geodesics → variable-star animation. Never cut: precision, star density, volumetrics, provenance honesty. |
| R11 | Solo-project attrition over ~18 months | High | High | **Structurally mitigated by §16.** The walking skeleton runs on day 2; Iteration 2 produces something genuinely beautiful in week ~3; every iteration ends runnable and screenshotted, so there is always a working app to enjoy and always a visible record of progress. Tours (post-prototype W6) make it enjoyable to *use*, not just to develop. | Any iteration boundary is a stopping point with a working app. The build log (§16.3) makes resumption cheap even after a multi-month gap — which is the specific failure mode of long solo projects. |
| R12 | Visual fidelity of volumetrics doesn't match reference imagery | Medium | Medium | Validate against real Hubble/ESO/JWST images of specific objects in P0-S3 and M5. Line-ratio physics (§H3) does most of the work. | Accept stylisation for a small set of "hero" objects while keeping the physics-based path as default. |

---

## 19. BUILD-SESSION OPERATING INSTRUCTIONS

For the engineer (human or AI) picking this up cold:

0. **If work has already started, execute the resume protocol in §16.3.3 first.** Read `docs/build/STATUS.yaml`, the last three `BUILDLOG.md` sessions, then *run the app and the benchmarks* before touching anything. Trust nothing that is not verified.
1. **Read §1, §2, §3 in full.** Do not skim §3 — it contains the three decisions that cannot be changed later without a rewrite. Then read §16.1 (build philosophy) and §16.3 (build log) — these govern *how* you work, not just what you build.
2. **Do not write application code before P0-S1.** The precision architecture is spike #1 for a reason. Everything else is downstream of it.
3. **Work milestone by milestone, spike by spike.** Each has a numeric exit criterion. Measure it, record it in `docs/perf_budgets.md` or the validation report, and write an ADR if you deviate.
4. **When this plan and reality disagree, reality wins — but record the deviation.** `docs/adr/NNNN-title.md`, one page: context, decision, consequences. Number them sequentially and never delete one.
5. **Check §2.2 before adding anything.** The most likely failure of this project is not a hard technical problem; it is scope.
6. **Provenance honesty is not optional.** Under D9, an object that renders must be traceable to data or to a calibrated model, and the UI must say which. This is the app's integrity.
7. **The debug/performance overlay ships from week 1.** An unobservable adaptive renderer cannot be built or debugged.
8. **Keep the Python ETL and the Rust runtime separated by the binary artifact contract.** The temptation to "just read the CSV in Rust" will cost a month. Resist it.
9. **Pin dependency versions** in `Cargo.lock` (commit it) and in `etl/uv.lock` or `requirements.txt`. wgpu moves fast and breaks things; a mid-project wgpu major-version bump is not a gift.
10. **Before starting:** (a) GPU confirmed — NVIDIA TITAN RTX; see §1.1.1 for the performance plan of record and §6.2 for the budget. (b) Disk confirmed — **200 GB free NVMe**; §8.4.1 is a hard constraint and the streaming-partitioned ETL is mandatory, not an optimisation. (c) Region confirmed — **Orion**, §16.2, with the exact cone parameters. (d) **Fire the Gaia TAP cone query before writing any code** — it is day 0 of the critical path and runs unattended overnight. Have the tiny Pleiades fallback fixture ready.
11. **`[MUST]` Update the build log every session, without exception.** §16.3. A session that does not end with a `BUILDLOG.md` entry, a rewritten `STATUS.yaml`, a `measurements.csv` row, a screenshot, and a commit **did not finish**. This project will be built across many sessions and possibly months apart; the log is the only thing that makes resumption cheap.

---

## APPENDIX A — CONDENSED BUILD BRIEF

*Paste this into a fresh session as the opening context, then hand over `PLAN.md`.*

```
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
```

---

## APPENDIX B — CRATE AND DEPENDENCY LIST

`[VERIFY]` all versions at build time. wgpu majors roughly quarterly (v29.0.0 Mar 2026, v30.0.0 Jul 2026) and Bevy is at 0.20 (Oct 2026) — irrelevant here since we are not using Bevy, but it indicates ecosystem churn, hence: **pin wgpu and do not upgrade mid-project.**

| Purpose | Crate | Notes |
|---|---|---|
| GPU | `wgpu` | Pin the version. Enable `BINDLESS` if used; check ` Features` for atomic-float support in P0-S2. |
| Windowing/input | `winit`, `raw-window-handle` | |
| Debug/settings UI | `egui`, `egui-wgpu`, `egui-winit` | Settings + debug overlay only (D10 exception). |
| Linear algebra | `glam` (f32/f64) | Prefer `glam` over `nalgebra` for hot paths — better codegen, simpler API. Use `DVec3`/`DMat4` for the CPU-side double math. |
| Handles/maps | `slotmap`, `generational-arena` | Body/entity storage. |
| Zero-copy | `bytemuck`, `static_assertions` | `#[repr(C)]` + `Pod` + size asserts. |
| Serialization | `serde`, `toml`, `rmp-serde` or `rkyv` | `rkyv` for zero-copy artifact loading; TOML for config. |
| Compression | `zstd`, `lz4_flex` | zstd for on-disk leaf payloads, lz4 for streaming decompression latency. |
| Async I/O | `tokio` (rt-multi-thread, io-util, fs, sync) | Streaming thread pool. |
| Spatial | `rstar` (R-tree), `bitvec` | CPU-side queries and visibility bitsets. |
| FFT | `rustfft` | Light-curve analysis, convolution. |
| RNG/noise | `rand`, `rand_xoshiro`, `noise` | **Seeded and deterministic** — `rand_xoshiro` with an explicit per-node seed (§8.7). Never `thread_rng` in content generation. |
| Search | custom trie, or `tantivy` | Custom preferred (§12.1). |
| Time | `time`, `chrono` | Calendar↔JD at the UI boundary only; internal time is `f64` days since J2000 TDB. |
| Error/logging | `thiserror` (libs), `anyhow` (app), `tracing`, `tracing-subscriber` | |
| Profiling | `puffin`, `puffin_egui` | CPU flamegraphs in-app. |
| Config | `serde` + `toml` + a versioned schema struct | Migration on load. |
| CLI | `clap` | `--reset-config`, `--bench-scene`, `--refresh-fixtures`, `--validate-data`. |
| HEALPix | `healpix` crate `[VERIFY]` availability/maturity, else vendor a minimal implementation | Survey layers and sky bucketing. |
| FITS | `fits-io` or `fitsio-pure` (pure Rust, no C deps) | Reading survey FITS in the ETL-adjacent tooling; the Python ETL is the primary path. |
| Ephemeris | `celestial-ephemeris` `[VERIFY]` completeness, else vendor truncated VSOP87/ELP tables | §9.2. |
| Text shaping | `cosmic-text` + a glyph-atlas baker (offline tool) | For the SDF font atlas in `assets/`. |
| Benchmarking | `criterion` | CPU-side benches. |
| Image I/O | `image`, `webp` | Imagery tiles. |
| Plots | `plotters` or `egui_plot` | Light curves and orbit plots in info panels. |
| License/advisory | `cargo-deny`, `cargo-audit` | CI. |

**Python ETL:** `astropy`, `astroquery`, `healpy`, `duckdb`, `polars`, `pyarrow`, `numpy`, `scipy`, `tqdm`, `requests`, `pyyaml`, `reproject`, `photutils`. Managed with `uv` for a lockfile.

---

## APPENDIX C — DATA SOURCE CATALOGUE (DETAIL)

Format: **Name** — provider — access — format — approx. size — license — notes.

**Astrometry / stars**
- **Gaia DR3** — ESA/DPAC — bulk CSV from the Gaia Archive (`archives.esac.esa.int/gaia`) or mirrors (ARI Heidelberg `gaia.ari.uni-heidelberg.de`, CDS `cdsarc.cds.unistra.fr`, NADC China) — CSV/Parquet/VOTable via TAP (ADQL) — ~1 TB compressed, ~2 TB unpacked — CC BY-NC 3.0 IGO — 1.47 B sources; epoch 2016.0; use `source`, `gaia_source`, `distance` (Bailer-Jones), `astrophysical_parameters`, `variability_summary`, `nss_two_body_orbit`, `xp_sampled`, `qso`, `galaxy`.
- **Gaia DR4** — as above — **2 December 2026** — ~2.8 B sources, new data model, own credit line and DOI. Draft data model already published; astrometric time-series pre-released.
- **HYG v3** — Astronexus — GitHub CSV — ~30 MB — permissive — 120 k stars with names. **Ingest this first.**
- **RECONS nearest-1000**, **Gliese/Jahreiß** — VizieR — small — public.
- **Washington Double Star (WDS)** — USNO — VizieR — ~147 k pairs.
- **Bailer-Jones geometric/EDR distances** — published with each Gaia DR — use instead of `1/parallax`. `[MUST]`

**Object identity / classification**
- **SIMBAD** — CDS — TAP sync/async, REST, sesame — ~5 GB relevant subset — CDS terms — the authority for identity and `otype`.
- **VizieR** — CDS — hosts essentially every catalog below — per-catalog licenses.
- **NASA/IPAC NED** — IPAC — REST/TAP — extragalactic.
- **IAU WGSBN / MPC naming citations** — MPC — text/DB.

**Deep sky**
- **Revised NGC/IC** (Sinclair, Tsvetanov/Steinicke) — ~13 000 objects.
- **Messier** (110), **Caldwell** (109), **Herschel 400**.
- **Sharpless** (313), **Barnard** (366), **Lynds dark** (1 794) / **Lynds bright** (913), **van den Bergh** (~150), **RCW** (~180), **LBN** (~700), **MWC** (~11 000 multi-class).
- **Strasbourg-ESO PN catalogue** — CDS — ~1 570 confirmed + candidates, with morphology.
- **SNRcat** (Green; Ferrand & Bardwadhi 2022 update) — ~294 Galactic SNR with diameters, ages, shell/filled/composite classes.
- **Harris catalogue of Galactic globular clusters** (2010 revision) — VizieR `VII/202` — 158 objects with `[Fe/H]`, `V`, `r_c`, `r_h`, `μ_V`, velocity.
- **Cantat-Gaudin et al.** open-cluster catalogues (Gaia DR2/DR3 based) — VizieR — ~2 000 clusters **with member lists**.
- **Dias et al. (Bica)** — ~2 000 open clusters. **MWSC** — 3 000+.
- **Solomon et al. 1987** — ~300 GMCs with mass/size/velocity. **Ellsworth-Bowers et al. 2013** — ~500 clouds.

**Dust / ISM**
- **Edenhofer et al. 2024** — Zenodo record 10658339 — parsec-scale 3D extinction to 1.25 kpc, from Gaia — CC BY 4.0 `[VERIFY]`.
- **Dharmawardena et al. 2024** — MNRAS 532, 3480 — all-sky 3D dust density and extinction to 2.8 kpc.
- **Lallement et al. Gaiamaps3D / HD 2019-2022** — VizieR.
- **Bayestar19** (Green et al.) — Pan-STARRS, δ > −30°, to ~4 kpc — VizieR.
- **Planck 857 GHz dust** — ESA — FITS — all-sky column density.
- **Marshall et al. 2009** — 2MASS-based Galactic-plane extinction map.
- **HI4PI** (21 cm, LAB + GASS) — FITS — all-sky.
- **Haslam 408 MHz** (Remazeilles 2014 destriped) — FITS.
- **Fermi LAT** all-sky — HEALPix FITS, multiple bands.
- **ROSAT** all-sky X-ray; **eROSita** public releases.
- **IRAS** (IRIS) 12/25/60/100 µm; **COBE/DIRBE**; **Planck** 353/545 GHz.

**Imagery (survey tiles)**
- **DSS2** — STScI — via Aladin HiPS or on-demand cutouts — attribution to POSS/UK Schmidt/AAO.
- **2MASS** (J/H/K) — IPAC — HiPS available.
- **WISE / AllWISE / unWISE** — IPAC — HiPS available.
- **GALEX** (UV), **SDSS** (optical, DR17), **DECaPS** — HiPS where available.
- **Planck SMICA** CMB — ESA.
- `[MUST]` **Use HiPS** (Hierarchical Progressive Surveys) as the tiling scheme — it is a HEALPix-based multi-resolution tile pyramid, is exactly the right data structure for this app, has published tooling, and is what Aladin/CDS already serve. Do not invent a tiling scheme.

**Solar System**
- **JPL Horizons** — REST/batch — authoritative; use offline to generate validation fixtures (§17.1).
- **JPL Small-Body Database (SBDB)** — API + bulk orbital-elements CSV (~2 GB) — ~1.4 M asteroids, ~4 000 comets, with H/G.
- **MPCORB.DAT** — Minor Planet Center — ~1.4 M orbits.
- **CNEOS / Sentry** — impact risk.
- **PDS / USGS Astropience & Solar System Treks** — global albedo mosaics (Moon/LRO, Mars, Mercury/MESSENGER, Venus/Magellan, Galileans, Titan, Pluto/New Horizons) — public domain.
- **JPL shape models** (OBJ/PLY) for ~200 small bodies.
- **VSOP87 / ELP2000-82B** coefficient tables — public domain (Bureau des Longitudes).
- **DE440/DE441 SPK** — JPL — `[MAY]` future accuracy upgrade (§9.2 escape hatch).

**Exoplanets / host stars**
- **NASA Exoplanet Archive** — `exoplanetarchive.ipac.caltech.edu` — TAP + CSV — `ps` (cumulative, > 6 000 planets), `pscomppars`, host-star parameters.
- **Open Exoplanet Catalogue** — GitHub single CSV — good provenance.
- **Exoplanet.eu** — Paris Observatory.
- **TESS TOI** — NASA.
- **SWEET-Cat**, **PASTIS** — high-precision host-star parameters.

**Galactic / extragalactic structure**
- **MWPOT18** (galpy, Bovy 2015) or **McMillan 2017** — Galactic potential.
- **BeSSeL** (Reid et al.) — VLBA maser parallaxes and proper motions for spiral-arm tracers — VizieR.
- **Vallée spiral-arm model**; **Hou & Han 2014**.
- **GRAVITY Collaboration** R₀ measurements; **Schönrich et al. 2010** solar peculiar velocity.
- **McConnach Local Group catalog** — ~100 members with structural parameters.
- **HyperLeda / LEDA** — galaxy morphology, Sérsic parameters, distances.
- **NASA-Sloan Atlas**, **2MRS**, **Cosmicflows-4**, **Huchra/2M++**.
- **van der Marel et al.** M31 proper motion and the MW–M31 encounter.

**Time domain / transient**
- **ATNF Pulsar Catalogue** — ~3 400 with P, Ṗ, DM, beam geometry.
- **Open Supernova Catalog** — ~2 000 with light curves.
- **Open Gamma-Ray Burst Catalog**; **Fermi GBM catalog**.
- **FRBCAT**; **TNS FRB list**; **CHIME/FRB catalog**.
- **GWTC-1/2/3/4** (LVC + KAGRA); **Gravitational Wave Open Science Center**.
- **AAVSO VSX** (~800 k variables); **GCVS**; **Gaia `variability_summary` + Cepheid/RR Lyrae tables**.
- **SILSO** sunspot numbers.

**Stellar physics**
- **Harre & Heller 2021**, *Digital colour codes of stars*, Astron. Nachr. 342, 578 — the magnitude/colour → display-colour table. `[MUST]`
- **Pickles (1998)** stellar spectral flux library; **Calspec**.
- **SVO Filter Profiles Service** — passbands and zero points.
- **MIST** (Dotter 2016) and **PARSEC** (Bressan 2012) isochrones.
- **Fitzpatrick (1999)** and **Cardelli–Clayton–Mathis (1989)** extinction laws.
- **Claret limb-darkening coefficient tables**.

---

## APPENDIX D — GLOSSARY

| Term | Meaning here |
|---|---|
| **ADQL** | Astronomical Data Query Language; SQL dialect for TAP services. |
| **Archetype** | A renderer code path + geometry/shape template. Many taxonomy classes share one archetype. |
| **Besançon model** | Robin et al. (2003) Galactic stellar population synthesis model; the basis for procedural star synthesis. |
| **Biller-Jones distance** | Bayesian photometric/astrometric distance estimate published with each Gaia DR. Use instead of 1/parallax. |
| **DSO** | Deep-sky object. |
| **EDR** | Early Data Release. |
| **Froxel** | A frustum-aligned voxel; the unit of a volumetric scattering grid. |
| **GSP-Spec / GSP-Phot** | Gaia Stellar Parameter pipelines (spectroscopic / photometric). |
| **HiPS** | Hierarchical Progressive Surveys — a HEALPix-based multi-resolution image tiling standard. |
| **ICRF / ICRS** | International Celestial Reference Frame/System; the canonical inertial frame. |
| **LDI** | Level of detail. Here: physical (magnitude-based), not distance-based. |
| **Morton order / Z-order** | Space-filling curve used to sort octree contents for locality. |
| **OoM** | Orders of magnitude. |
| **PNe / PN** | Planetary nebula(e). |
| **Reversed-Z** | Depth buffer convention mapping near→1, far→0 for precision. |
| **Sérsic index** | Parameter describing a galaxy's surface-brightness profile concentration. |
| **SNR** | Supernova remnant (context!) or signal-to-noise ratio. |
| **Split-double / double-single** | Representing an f64 as two f32s (hi + lo) to recover precision on GPUs without native f64. |
| **SPICE / SPK** | NAIF's spacecraft ephemeris toolkit and kernel format. **Not used** (D4). |
| **TAP** | Table Access Protocol; the standard astronomy data service API. |
| **TDB / TT / TAI / UTC** | Barycentric Dynamical Time / Terrestrial Time / International Atomic Time / Coordinated Universal Time. |
| **VizieR** | CDS's catalog service hosting most astronomical tables. |
| **VOTable** | The standard XML table format for astronomy. |

---

## APPENDIX E — REFERENCES AND REFERENCE IMPLEMENTATIONS

**Techniques this plan depends on**

1. Schneegans, Kreskowski & Gerndt, *Smaller than Pixels: Rendering Millions of Stars in Real-Time*, Eurographics 2025 — [PDF](https://www.uni-weimar.de/fileadmin/user/fak/medien/professuren/Virtual_Reality/documents/publications/2025_Smaller_than_Pixels_Rendering_Millions_of_Stars_in_Real-Time.pdf). **The most directly relevant paper to this project.** Software rasterisation of 22 M in-frustum Gaia stars in ~8 ms at 4K on an RTX 4070 Super; perspective-correct solid-angle luminance; the `c/θ²` glare pyramid via mip levels weighted `1/2ⁱ` with bicubic filtering; measured glare costs of 0.74 ms (screen-space) and 1.38 ms (view-space asymmetric) at 4K. Open-source implementation in **CosmoScout VR**.
2. Schütz, Kerbl & Wimmer, *Software Rasterization of 2 Billion Points in Real Time*, Proc. ACM CGIT 5(3), 2022 — the point-cloud software-rasteriser foundation.
3. Karis, Stubbe & Wihlidal, *A Deep Dive into Nanite Virtualized Geometry*, SIGGRAPH 2021 — the canonical reference for GPU-driven culling, cluster LOD, and hierarchical visibility. Adapt the ideas; do not copy the mesh-centric structure.
4. Schieder & Eitzinger (and Hillaire's *A Scalable and Production Ready Sky and Atmosphere Rendering Technique*), *Optimisations for Real-Time Volumetric Cloudscapes* ([arXiv:1609.05344](https://arxiv.org/html/1609.05344v1)) — froxel volumes, temporal reprojection at reduced rate, blue-noise jitter, empty-space skipping.
5. Krawczyk, Myszkowski & Seidel, *Perceptual Effects in Real-Time Tone Mapping*, SCCG 2005 — the glare model.
6. Pattanaik, Tumblin, Yee & Greenberg, *Time-Dependent Visual Adaptation for Fast Realistic Image Display*, SIGGRAPH 2000 — auto-exposure adaptation.
7. Duiker, *Filmic Tonemapping for Real-Time Rendering*, SIGGRAPH 2006 course.
8. Sigg & Hadwiger, *Fast Third-Order Texture Filtering*, GPU Gems 2 — bicubic reconstruction for the glare pyramid.
9. Sagristà, Jordan, Müller & Sadlo, *Gaia Sky: Navigating the Gaia Catalog*, IEEE TVCG 25(1), 2018 — the octree LOD θ-criterion.
10. Schümann et al. and the Magnopus write-up, [How we render extremely large point clouds](https://www.magnopus.com/blog/how-we-render-extremely-large-point-clouds) — practical compute-based point-cloud pipelines.
11. Kitware, [Achieving Interactivity with a Point Cloud of 2 Billion Points](https://www.kitware.com/achieving-interactivity-with-a-point-cloud-of-2-billion-points-in-a-single-workstation/) — manual rasterisation with a maintained depth buffer.
12. Harre & Heller, *Digital Colour Codes of Stars*, Astronomische Nachrichten 342, 578 (2021).
13. Edenhofer et al., *A parsec-scale Galactic 3D dust map out to 1.25 kpc from Gaia*, A&A 685, A82 (2024) — [Zenodo 10658339](https://zenodo.org/records/10658339).
14. Dharmawardena et al., *All-sky three-dimensional dust density and extinction maps of the Milky Way out to 2.8 kpc*, MNRAS 532, 3480 (2024).
15. Robin et al. 2003 (Besançon); Bovy 2015 (galpy/MWPOT18); McMillan 2017 (Galactic potential).
16. van der Marel et al. — the Milky Way–Andromeda encounter.

**Reference implementations to study (do not copy; note licenses)**

- **Gaia Sky** (MPL-2.0, [github.com/langurmonkey/gaiasky](https://github.com/langurmonkey/gaiasky)) — Java/libGDX. Its public documentation of the star-rendering magnitude→pseudo-size pipeline and the octree LOD θ-criterion is directly useful.
- **`gaiasky-catgen`** ([codeberg.org/gaiasky/gaiasky-catgen](https://codeberg.org/gaiasky/gaiasky-catgen)) — **a Rust re-implementation of the Gaia Sky LOD catalog generator**, MPL-2.0. This is the single most valuable prior artifact for §8: read it before writing the octree builder. Its published binary LOD format (`metadata.bin` + `particles_*.bin`) is a proven design.
- **CosmoScout VR** (DLR, open source) — the implementation of reference #1; SPICE-based modular solar system.
- **Celestia** — GPL; billboard-based star rendering and its documented limitations are instructive about what *not* to do.
- **Stellarium** — GPL; the first release to use Gaia DR3 was March 2025; study its catalog chunking and survey (DSS/TOAST) layer handling.
- **SpaceEngine** — proprietary; the benchmark for the *experience* (seamless zoom, procedural synthesis, diegetic UI). Study screenshots and the developer's technical write-ups, not code.
- **Aladin / CDS HiPS tooling** — the reference for progressive survey tiling.
- **Partiview / Digital Universe** — academic galaxy-scale visualisation.

**Ecosystem facts recorded at planning time (2026-10-23) — re-verify before use**

- **Gaia DR4 releases 2 December 2026**, ~2.8 B sources, new data model; DR5 will follow, covering the full 10.5-year mission.
- **Gaia data is CC BY-NC 3.0 IGO** (non-commercial); commercial use requires ESA's archive terms and conditions.
- **wgpu** majors: v29.0.0 (Mar 2026), v30.0.0 (Jul 2026). Bindless is available via `Features::BINDLESS` on DX12/Metal/Vulkan 1.2+. Hardware ray tracing remains incomplete.
- **Bevy** is at 0.20 (Oct 2026) — not used here (D5).
- **DLSS is not available through wgpu.** FSR2 (vendor-neutral, compute-shader) and XeSS are the portable options; XeSS runs on Turing via **DP4a**, but the DP4a path is measurably weaker than XMX XeSS or DLSS — **FSR2 is the default**.
- **TITAN RTX confirmed as the reference target**: TU102, 4 608 CUDA cores / 36 SMs, 576 Tensor Cores, 72 RT Cores, 288 TMUs, 96 ROPs, 24 GB GDDR6 / 384-bit / 672 GB/s, 1770 MHz boost, ~16.3 TFLOPS FP32, PCIe 3.0, NVLink-capable (2 cards → 48 GB). Turing = SM 7.5: DP4a and descriptor indexing yes; mesh shaders, Int64 atomics, FP8, DLSS3 frame-gen no. **6 MB L2 vs Ada's 48 MB** is the main reason an atomic-heavy software rasteriser is proportionally slower here than raw TFLOPS implies.
- Pure-Rust FITS readers exist (`fits-io`, `fitsio-pure`) — no cfitsio dependency needed.
- NASA Exoplanet Archive cumulative table: > 6 000 confirmed planets.

---

## END OF PLAN

**Open items — status as of 2026-10-23:**

1. ~~The actual reference GPU~~ — **RESOLVED 2026-10-23: NVIDIA TITAN RTX, 24 GB.** See §1.1.1 for the revised performance plan of record.
2. ~~Available free NVMe space~~ — **RESOLVED: 200 GB free NVMe.** Host is 96 GB RAM / 32 threads / 200 GB NVMe, which is ample for compute but *tight for data*. Consequences are significant and are specified in **§8.4.1** (streaming-partitioned ETL is mandatory; the full Gaia CSV dump cannot be downloaded) and **§8.5** (tiers revised; T3 Full is not installable here).
3. ~~Prototype region~~ — **RESOLVED: Orion**, exact cone parameters in §16.2.
4. ~~Build sequencing~~ — **RESOLVED: prototype first, then iterate.** §16 restructured accordingly.
5. **Remaining open: confirm the Gaia TAP query can be started.** §16.2 gives the exact ADQL; day 0 of the critical path depends on it, and the Pleiades fallback fixture must be built alongside it so a throttled download never blocks Iteration 1.
4. Whether "no scripting, no cinematics" (D11) should be revisited given that *tours* (§12.3) are effectively a non-scripted cinematic system and are planned to be included — this is a small contradiction in the decision set, resolved here in favour of including tours because they are the highest-value-per-cost feature in the plan.
