# 0002. Software rasterisation over billboard/point stars

**Date:** 2026-10-24 · **Status:** Accepted
**Iteration:** 2 · **Session:** 007 · **Commit:** `428360c`

## Context
The It.0/It.1 star pass is instanced additive billboards ("point stars") — a
walking-skeleton renderer, never intended to survive (`PLAN.md` §7.3 specifies
its replacement). Two measurements make the case concrete:

- Sandbox (llvmpipe, ADR-0010): `t0_orion_m13` with the s006-complete dataset
  (11.03 M stars, 1.5 M resident) costs **215–230 ms/frame at 2560×1440**, and
  the cost is additive-overdraw-bound: overlapping billboard quads blend per
  pixel, so frame time scales with drawn-star footprint, not star count
  (10 k stars = 22–24 ms; 1.5 M = ~230 ms; 3 M = ~610 ms in fly-test frames).
- `PLAN.md` §6.2 allots the star rasterisation **5.5 ms for 10 M stars at
  1440p** on the TITAN RTX, and the §6.2 budget already *assumes* the §7.3
  approach: its extrapolation comes from the Eurographics 2025 result
  (22 M stars @ 4K in 8 ms on a 4070 Super with a compute-tile rasteriser).
  Billboard overdraw has no published path to that number, and the §6.2 table
  is over budget by 1.9 ms even with the rasteriser assumed.

§7.3 specifies the required behaviour: compute shader, one workgroup per 8×8
screen tile; per-tile star lists from a binning pass; analytic disc/pixel
overlap coverage (this is what kills shimmer); perspective-correct solid
angle (`[MUST]` — edge stars cover less solid angle and must be drawn
proportionally brighter or wide-FOV views are wrong); depth test against the
half-res prepass; atomic-float accumulation, or per-tile u32 fixed-point with
a frame-global normalisation factor if float atomics are unavailable.

## Decision
Adopt the §7.3 compute-tile software rasteriser as THE star pass in Iteration
2, replacing the instanced additive billboards. The billboard path stays
compiled and selectable (scene/CLI flag) until the rasteriser passes its
visual + numeric gates (golden shots + photometric unit tests + the §17
validation set), then is deleted in the same iteration. Implementation order
inside It.2: HDR float target and tonemap chain first (the rasteriser
accumulates radiance, not display colour), then tile binning, then the tile
shader with analytic coverage and perspective-correct solid angle, then the
u32 fixed-point fallback where float atomics are missing (conditional
ADR-0007).

## Consequences
- An HDR (float) render target becomes mandatory before the rasteriser lands;
  exposure/tonemap (§6.2 pass 16) moves from "later" to prerequisite.
- The frame graph gains a binning pass and a compute pass; the 7 named pass
  slots grow — per-pass GPU timestamps (s007 double-buffered timers) already
  cover arbitrary slot counts.
- Sandbox perf numbers for the rasteriser are **correctness evidence only**
  (ADR-0010): llvmpipe compute throughput says nothing about the Titan budget.
  Gates judged here: golden-image deltas, photometric tests, determinism.
- On GLES/llvmpipe float32 atomics are not guaranteed; expect to exercise the
  u32 fixed-point path in the sandbox first (that is a feature: it proves the
  fallback before the target machine ever needs it).
- Shimmer and overdraw regressions become measurable: the fly-test CSV
  (per-frame drawn stars) plus golden shots at fixed seeds are the harness.

## Alternatives considered
- Keep billboards, add alpha-to-coverage/MSAA: rejected — overdraw scaling and
  point-sample shimmer remain; no analytic coverage; no path to §6.2 budget.
- GPU point sprites: rejected — driver point-size limits, no disc coverage,
  no perspective-correct solid angle.
- Defer the rasteriser to a post-prototype wave: rejected — §6.2 does not
  close without it and It.2 ("BEAUTIFUL") is the designated iteration;
  deferring pushes the project's core visual identity out by months.
