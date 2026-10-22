# DAY 0 — Fire this before writing any application code

**Why first:** the Orion cone query is 14–20 GB and runs unattended overnight. It is the
critical path for Iteration 1. Every day it is delayed is a day added to the schedule.
Spec: `PLAN.md` §16.2, §8.4.1.

---

## 0. Pre-flight (5 minutes)

```bash
# Confirm free NVMe. HARD LIMIT: 200 GB. The ETL must refuse to start below 40 GB free.
df -h /path/to/workspace        # Windows: Get-PSDrive C

# Confirm Python + tooling
python3.12 --version
uv --version || pip install uv
```

Create the workspace layout:

```
galaxia/
├── etl/raw/orion/          # transient — DELETED after each partition (§8.4.1 rule 1)
├── etl/norm/              # zstd parquet intermediates
├── etl/build/             # baked artifacts → this is the T0 dataset (~1.5 GB)
└── etl/reports/           # validation output
```

---

## 1. Smoke test — 2 minutes, proves credentials/endpoints/reachability

Run this **first**. If it fails, the overnight run would have failed too, and you have
lost nothing but two minutes.

```bash
curl -s --data-urlencode REQUEST=doQuery \
       --data-urlencode LANG=ADQL \
       --data-urlencode FORMAT=csv \
       --data-urlencode QUERY="SELECT TOP 10 source_id, ra, dec, phot_g_mean_mag FROM gaiadr3.gaia_source WHERE CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', 83.82, -5.39, 0.5)) = 1 AND phot_g_mean_mag < 12 ORDER BY phot_g_mean_mag" \
  https://gea.esac.esa.int/tap-server/tap/sync
```

Expect 10 rows including Betelgeuse and Rigel. If it times out or 503s, switch mirror:

| Mirror | Endpoint | Notes |
|---|---|---|
| ESA Gaia Archive (primary) | `https://gea.esac.esa.int/tap-server/tap/` | Authoritative. Can throttle under load. |
| ARI Heidelberg | `https://gaia.ari.uni-heidelberg.de/tap/` | Fast, reliable, full DR3 mirror. |
| CDS Strasbourg | `https://tap.cds.unistra.fr/tap/` | Also hosts most VizieR catalogs — useful later. |
| NADC China | `https://nadc.china-vo.org/tap/` | Often the fastest from East Asia. |

---

## 2. Count first — 1 minute, tells you the real row count

```sql
SELECT COUNT(*) AS n
FROM gaiadr3.gaia_source
WHERE CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', 83.82, -5.39, 20.0)) = 1
  AND phot_g_mean_mag < 20.0
```

Expected: **35–50 M**. If it comes back over ~80 M, tighten to `G < 19.5` or shrink the
cone to 18° — the prototype does not need every faint source, and raw disk is the
binding constraint (§8.4.1).

Also grab the bright-star subset separately — it is tiny, fast, and immediately useful
for Iteration 0's hardcoded skeleton:

```sql
SELECT TOP 20000 source_id, ra, dec, phot_g_mean_mag, bp_rp, parallax
FROM gaiadr3.gaia_source
WHERE CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', 83.82, -5.39, 20.0)) = 1
  AND phot_g_mean_mag < 10.0
ORDER BY phot_g_mean_mag
```

---

## 3. The real run — **streaming-partitioned** (mandatory, §8.4.1)

**Do NOT issue one 40 M-row query.** It will fail, or produce a 20 GB file you cannot
re-run cheaply, and it violates the peak-disk rule.

Partition the cone by HEALPix. At `Nside = 32` a 20° cone covers roughly 500–550 pixels;
each pixel holds ~70–90 k rows (~30 MB CSV) — small enough to process in memory, delete
immediately, and resume from if the network drops.

```python
# etl/fetch_orion.py — DAY 0 downloader. Streaming-partitioned per PLAN §8.4.1.
#
# Usage:  uv run python etl/fetch_orion.py --nside 32 --gmax 20.0 --workers 4
#
# Rules this enforces:
#   * free-disk gate: refuses to start below 40 GB free (§8.4.1 rule 5)
#   * per-partition: fetch -> write raw csv -> (caller processes) -> delete raw (rule 1)
#   * resumable: skips partitions whose baked output already exists
#   * retries with backoff and automatic mirror failover
#   * writes etl/reports/fetch_manifest.json (provenance: endpoint, query, timestamp,
#     row count, sha256) for lineage.py

CONE_RA, CONE_DEC, CONE_R = 83.82, -5.39, 20.0   # PLAN §16.2 — ICRS degrees
G_MAX = 20.0
EPOCH = 2016.0                                    # Gaia DR3 reference epoch
TABLE = "gaiadr3.gaia_source"                     # ← the ONLY place a Gaia table or
                                                  #    column name may appear (D7).
                                                  #    DR4 = change this + the mapping.

COLUMNS = [
    "source_id", "ra", "dec", "parallax", "parallax_over_error",
    "pmra", "pmdec", "radial_velocity", "radial_velocity_error",
    "phot_g_mean_mag", "phot_bp_mean_mag", "phot_rp_mean_mag",
    "bp_rp", "astrometric_excess_noise", "phot_variable_flag",
    "teff_val", "logg", "mh", "ag_gspphot", " ebpminrp_gspphot",
    "classprob_dsc_combmod_star", "classprob_dsc_combmod_quasar",
    "classprob_dsc_combmod_galaxy", "in_qso_candidates", "in_galaxy_candidates",
    "non_single_star", "has_epoch_photometry", "solution_id",
]
```

Key implementation notes for whoever writes this:

- Use **async TAP jobs** (`/tap/async`), not sync. Sync has row limits and long queries
  will be killed. `astroquery.gaia.Gaia.launch_job_async()` handles the polling.
- `--workers 4` is a sane ceiling. More than ~6 concurrent jobs against the ESA endpoint
  invites throttling or a block. Be a good citizen — this is a shared science archive.
- **Checkpoint every partition.** Record `healpix_index → sha256 → row_count` in
  `fetch_manifest.json`. A 14 GB download that fails at 90 % and cannot resume is a
  lost weekend.
- Select `solution_id` — it pins the exact processing solution, which is what makes the
  result reproducible and comparable when DR4 arrives.
- `ag_gspphot` / `ebpminrp_gspphot` are the GSP-Phot extinction values. Take them where
  present; they are a useful cross-check against the 3D dust map (§7.5).

---

## 4. The fallback fixture — build this too, same day

**Never let a download be a schedule dependency twice** (§16.2). The Pleiades alone:

```sql
SELECT source_id, ra, dec, parallax, parallax_over_error, pmra, pmdec, radial_velocity,
       phot_g_mean_mag, phot_bp_mean_mag, phot_rp_mean_mag, bp_rp, phot_variable_flag
FROM gaiadr3.gaia_source
WHERE CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', 56.75, 24.12, 2.0)) = 1
  AND phot_g_mean_mag < 18.0
```

≈ 1 M rows, ≈ 400 MB CSV, ≈ 30 MB baked, finishes in minutes. Small enough to commit
adjacent to the test suite as a golden fixture. If the Orion run is still going, or
throttled, or corrupt, **Iteration 1 proceeds on this instead.**

---

## 5. While that downloads — the parallel workstream

Do not idle. Day 0–2 is also:

- `cargo init` the workspace per `PLAN.md` §4.1. **Pin `wgpu`** in `Cargo.toml` and
  commit `Cargo.lock` — wgpu majors land roughly quarterly and a mid-project bump is
  not a gift.
- Window + swapchain + a triangle.
- **WGSL hot-reload.** Build this on day 1. You will recompile shaders a few hundred
  times; every second of latency there compounds across months.
- GPU timing query sets and the debug overlay. §16.6 rule 4: no feature is done until
  it has a measured number, and you cannot measure what you did not instrument.
- `BUILDLOG.md`, `docs/build/STATUS.yaml`, `docs/build/measurements.csv`,
  `docs/adr/INDEX.md` — templates are in `templates/` alongside this file. Copy them
  in and **write session 001 before you write any Rust.**

---

## 6. Licence note (do this once, now)

Gaia data is **CC BY-NC 3.0 IGO**. This is fine — D1 says the project is personal and
never distributed. But record the attribution at fetch time rather than reconstructing
it later:

> This work has made use of data from the European Space Agency (ESA) mission *Gaia*
> (https://www.cosmos.esa.int/gaia), processed by the *Gaia* Data Processing and Analysis
> Consortium (DPAC, https://www.cosmos.esa.int/web/gaia/dpac/consortium). Funding for the
> DPAC has been provided by national institutions, in particular the institutions
> participating in the *Gaia* Multilateral Agreement.

Plus the DR3-specific credit line and DOI from <https://www.cosmos.esa.int/web/gaia/dr3>.
`etl/lineage.py` should generate `data/CREDITS.md` from `fetch_manifest.json`
automatically — hand-maintained credit files rot (§13.1).
