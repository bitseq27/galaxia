# 0004. Orion as the prototype region

**Date:** 2026-10-23 · **Status:** Accepted
**Iteration:** 0 · **Session:** 001 · **Commit:** `654bad1` (data confirmation)

## Context
PLAN §16.2 pre-selected the Orion cone (α=83.82°, δ=−5.39° ICRS, r=20°, G<20) as the
prototype region and estimated **35–50 M rows / 14–20 GB raw CSV**. The trigger for
this ADR was the day-0 measurement: the archive's own COUNT for exactly that cone is
**15,573,949 rows** (job `b3a87cff-ceb8-11f1-9d98-bc97e148b76b-O`, 2026-10-23).

## Decision
Orion stays the prototype region. The row-count estimate in PLAN §16.2/BUILD_BRIEF is
superseded by the measured 15.57 M (raw CSV ≈ 6 GB, baked ≈ 0.5 GB at the §8.4 32 B/star
budget). The streaming-partitioned ETL design (§8.4.1) is retained unchanged — the
peak-disk rule binds on partition residency, not total volume, and the margin it
buys is free.

## Consequences
- The overnight Orion download is ~3× cheaper than planned; a full-cone fetch now
  fits inside a 9 GB disk envelope **if** streamed (peak = one ~30 MB partition +
  baked output). The build sandbox can therefore validate the fetcher end-to-end.
- T0 baked size drops from ~1.1–1.6 GB to ~0.5 GB, freeing VRAM/streaming headroom
  for It.1 experiments.
- The faint-end density in the cone is lower than the plan assumed; the It.2 gate
  (10 M visible stars) may need the synthetic "enhanced visibility" population
  earlier than planned to be exercised — noted, not decided.

## Alternatives considered
- Re-centre or shrink the cone to hit the old row-count target: rejected — the cone
  was chosen for its object inventory (M42/M43, Trapezium, Barnard's Loop, the
  Horsehead, real cluster member lists) and dust-map coverage, not for row count.
