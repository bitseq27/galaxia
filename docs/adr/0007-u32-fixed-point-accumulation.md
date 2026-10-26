# 0007. u32 fixed-point star accumulation (float atomics unavailable in the pinned stack)

**Date:** 2026-10-26 · **Status:** Accepted
**Iteration:** 2 · **Session:** 009 · **Commit:** `(see BUILDLOG s009)`

## Context
PLAN §7.3 specifies the tile rasteriser's accumulation as "atomic-float
accumulation, or per-tile u32 fixed-point with a frame-global normalisation
factor if float atomics are unavailable". PLAN §19 pre-allocated this ADR for
exactly that conditional. Session 009 probed the pinned stack (wgpu =30.0.1,
ADR-0010 backends):

- naga 30 validates `atomic<f32>` only under
  `Capabilities::SHADER_FLOAT32_ATOMIC`; wgpu exposes it as the native-only
  feature `wgpu-shader-float32-atomic`, advertised by the **Vulkan** backend
  only when `VK_EXT_shader_atomic_float` is present, and by Metal.
- The **GLES backend never advertises it** — so the build sandbox (llvmpipe,
  ADR-0010) can neither run nor test an f32-atomic accumulator, ever.
- On the target (TITAN RTX/Vulkan) availability depends on the driver
  exposing the EXT; it would have to be probed at runtime and the shader
  forked per backend.

## Decision
The star rasteriser accumulates **per-pixel per-channel u32 fixed-point
(Q10 radiance) with `atomicAdd`**, resolved into the s008 HDR float target by
a normalisation pass (divide by 2^10, add the background radiance). This is the
single accumulator on all backends; no f32-atomic shader variant is written.

## Consequences
- Deterministic, backend-independent, and fully testable in the sandbox —
  the photometric flux gates (BUILDLOG s009) run on llvmpipe.
- Q10 headroom: saturates at radiance ~4.19e6 per channel per frame — orders
  of magnitude above any physical scene value (brightest deposit ~14); a
  saturation clamp is still applied and counted for diagnostics.
- Cost: one extra fullscreen resolve pass (trivial next to the existing
  tonemap) and 4 B/channel accumulation storage (2560×1440×4ch×4 B = 59 MB).
- If target measurements ever show the accumulator bandwidth-bound, an f32
  variant behind `Features::SHADER_FLOAT32_ATOMIC` is a contained optimisation
  (same deposits, different atomic type) — record it as a new ADR superseding
  this one, do not fork silently.

## Alternatives considered
- f32 atomics where available + u32 fallback elsewhere: rejected — forks the
  hot shader into a path the sandbox cannot execute or gate (violates the
  "gates run where the work is tested" practice established in s006-s008),
  for an unmeasured gain.
- Per-tile local accumulation + merge (no global atomics): rejected for v1 —
  requires tile-local storage sizing and a merge pass; revisit only if
  atomic contention measures as a problem on the target.
- f16 accumulation: rejected — range/precision inadequate for radiance sums.
