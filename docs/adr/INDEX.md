# Architecture Decision Records — INDEX

One page per deviation from `PLAN.md`: **Context / Decision / Consequences.**
Numbered sequentially. **Never deleted, never renumbered.** If a decision is reversed,
write a new ADR that supersedes the old one and mark the old one `Superseded by NNNN`.

Specification: `PLAN.md` §16.6 rule 5, §19 rule 4.

---

## Template — `docs/adr/NNNN-short-name.md`

```markdown
# NNNN. <Title>

**Date:** YYYY-MM-DD · **Status:** Accepted | Superseded by NNNN | Deprecated
**Iteration:** N · **Session:** NNN · **Commit:** `<hash>`

## Context
<What forced the decision. Reference the PLAN.md section that is being deviated from,
or the new situation the plan did not anticipate. Two to six sentences.>

## Decision
<What was decided. One paragraph. Imperative, unambiguous.>

## Consequences
<What this makes easier, what it makes harder, what it forecloses, and what must now
be true elsewhere in the codebase. Be honest about costs.>

## Alternatives considered
<One line each, and why rejected. This is what stops the decision being relitigated
in six months by someone who does not remember.>
```

---

## Planned ADRs (pre-allocated by `PLAN.md`; write them when the decision is actually made)

| # | Title | Trigger |
|---|---|---|
| 0001 | Frame hierarchy and floating-origin rebasing | Iteration 3 — adopting §H1 |
| 0002 | Software rasterisation over billboard/point stars | Iteration 2 — adopting §7.3 |
| 0003 | **egui for the prototype UI, deferring the diegetic system (D10)** | Iteration 5 — deliberate shortcut; undone in wave W2 |
| 0004 | Orion as the prototype region | Day 0 — confirmed by product owner 2026-10-23 |
| 0005 | FSR2 as the default upscaler (DLSS unavailable via wgpu; XeSS DP4a weaker) | Iteration 2 |
| 0006 | Streaming-partitioned ETL under the 200 GB disk constraint | Iteration 1 — mandatory per §8.4.1 |
| 0007 | u32 fixed-point star accumulation *(only if atomic float is unavailable)* | Iteration 2, conditional |
| 0008 | Orion-only data scope for the prototype, T1+ deferred | Iteration 6 exit review |
| 0009 | Default render preset chosen from measurement | Iteration 6 exit review |

---

## Actual ADRs

| # | Title | Status | Session |
|---|---|---|---|
| [0002](0002-software-rasterisation.md) | Software rasterisation over billboard/point stars (§7.3) | Accepted | 007 |
| [0004](0004-orion-prototype-region.md) | Orion as the prototype region (measured count supersedes estimate) | Accepted | 001 |
| [0010](0010-sandbox-gles-build.md) | Build sandbox: Linux + GLES/llvmpipe instead of Windows/Vulkan | Accepted | 001 |
