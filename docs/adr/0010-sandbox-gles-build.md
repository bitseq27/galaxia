# 0010. Build sandbox: Linux + GLES/llvmpipe instead of the Windows/Vulkan target

**Date:** 2026-10-23 · **Status:** Accepted
**Iteration:** 0 · **Session:** 001

## Context
The target machine (PLAN §1.1) is Windows + TITAN RTX over Vulkan. The build session
runs in a Linux container with **1 GB RAM, 2 cores, ~9 GB disk, no display**, and
rustc is OOM-killed compiling `ash` (the Vulkan binding crate, ~1.5 GB RSS for type
check) even with debug info off. wgpu's `vulkan` feature therefore cannot be compiled
here at all, and egui-wgpu's default features silently re-enable it (`wgpu/default`),
as does any member crate leaving `galaxia-render`'s defaults on.

## Decision
The workspace splits wgpu backends behind cargo features:
`galaxia-render`/`galaxia-app` have `default = ["vulkan"]`, plus `gles` and `dx12`.
The sandbox builds with `--no-default-features --features gles` (Mesa llvmpipe via
EGL surfaceless), the Windows target builds with defaults (Vulkan). wgpu stays
pinned at =30.0.1 in both flavours; Cargo.lock is identical. `GpuContext::new`
selects only backends compiled in (cfg-gated), so runtime behaviour is
deterministic per build flavour. All screenshots/measurements from the sandbox are
produced headless via `--screenshot`/`--bench` (offscreen render target).

## Consequences
- Sandbox numbers (measurements.csv rows marked "sandbox lavapipe") are
  **correctness evidence only, never performance evidence**. Perf gates
  (PLAN §6.2) can only be judged on the TITAN RTX.
- GL backend lacks some Vulkan features; GPU timestamps exist but are unreliable
  on llvmpipe (reported ~0; headless bench falls back to CPU wall time when a GPU
  value is < 0.01 ms). Golden-image and astrometric validation are unaffected.
- Windowed code paths (winit event loop, egui overlay) compile but are UNTESTED in
  the sandbox (no display). First run on the target machine must be treated as
  integration, not verification.
- Feature unification is a trap: adding a dependency that enables `wgpu/default`
  (as egui-wgpu does) silently drags `ash` back in and OOMs the sandbox. CI (It.3)
  must build both flavours.

## Alternatives considered
- Bigger sandbox RAM: not available (container limit, no swap permission).
- Downgrade wgpu to a version with a smaller `ash`: rejected — pin is a plan
  decision (Appendix B), and the drift cost would recur at every upgrade.
- Drop the Vulkan backend entirely: rejected — it is the target's backend.
