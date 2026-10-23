#!/usr/bin/env python3
"""etl/fetch_cone_exact.py — session 006 EXACT-TILING cone fetcher (the 50k-cap fix).

WHY THIS EXISTS (s005 -> s006).  The archive silently caps TAP results at 50k
rows. s005's prescribed fix was sub-partitioned re-fetch via HEALPix children,
but s006 MEASURED that child chord-polygons do not tile the parent exactly
(n32_4702: parent polygon COUNT 60,792; 4 children SUM 60,789; complement
parent-minus-children 82 rows; child overhang 79 rows double-counted with the
neighbour). Chord polygons also cannot tile the CONE RIM exactly, so the
pixel-polygon scheme can never hit the completeness gate
"sum(rows+skipped) == 15,573,949 EXACTLY".

SCHEME.  Exact tiling by half-open (ra, dec) boxes intersected with the SAME
circle predicate that produced the gate count:
    CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', 83.82, -5.39, 20.0)) = 1
    AND dec >= lo AND dec < hi AND ra >= lo AND ra < hi
    AND phot_g_mean_mag < gmax
Every row in the cone satisfies exactly one box (plain column comparisons, no
spherical-polygon edge semantics anywhere), so the sum over leaves equals the
cone COUNT exactly, by construction. Adjacent boxes share the identical
formatted boundary string on both sides — no gap, no overlap. This is the
"RA bands" partitioning PLAN §8.4.1 rule 1 explicitly allows.

Subdivision: a leaf result at the >= 49,999 canary is TRUNCATED — split the
box along its longer axis (tie: ra) into two half-open halves and recurse
(floor: 12 levels). Children are fetched and CONCATENATED in ascending order
("a" half first), rows*32 == bytes exact. Deterministic on a frozen archive.

Contract (same as fetch_orion.py):
  etl/build/box_<path>.bin          32 B quantised records per leaf region
  etl/reports/fetch_manifest.json   {"scheme", "regions": {...}} — one entry
                                    per leaf (rows/skipped/baked_sha256/box),
                                    internal nodes record children paths
  resumable: leaf bin + manifest entry exist -> skip; --deadline lets chunked
  foreground runs exit cleanly between regions (sandbox rule: background
  processes do not survive between tool calls)
  raw CSV deleted immediately after quantising (§8.4.1 rule 1)
  non-ESA mirror results are async-COUNT-verified (other mirrors may cap
  below the canary — silent under-fetch guard)
"""
import argparse
import hashlib
import json
import math
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from fetch_orion import (CONE_RA, CONE_DEC, CONE_R, COLUMNS, RAW, BAKE,
                         REPORTS, CAP_CANARY, log, free_gb, sha256_file,
                         tap_sync, quantise_csv)

MANIFEST = os.path.join(REPORTS, "fetch_manifest.json")
COUNT_FILE = os.path.join(REPORTS, "day0_orion_count.txt")
DEC_START = math.floor(CONE_DEC - CONE_R) - 1.0   # -26.0 with margin
DEC_END = math.ceil(CONE_DEC + CONE_R) + 1.0      # +15.0 with margin
DEC_BAND = 1.0
RA_MARGIN = 0.05          # deg, on the analytic per-band ra extent
MAX_DEPTH = 12            # split floor: 46 deg / 2^12 ~ 0.011 deg boxes
FMT = "{:.6f}"


def circle_ra_halfwidth(dec):
    """Max |ra - CONE_RA| (deg) for points inside the cone at latitude `dec`.

    From cos(r) = sin(d0)sin(d) + cos(d0)cos(d)cos(dra).  Returns 0.0 for
    dec outside the cone's dec range.  Monotonically decreasing in
    |dec - dec0|, so the widest point of a band is the band's dec closest
    to the cone centre."""
    sd, cd = math.sin(math.radians(dec)), math.cos(math.radians(dec))
    if cd < 1e-12:
        return 0.0
    x = (math.cos(math.radians(CONE_R))
         - math.sin(math.radians(CONE_DEC)) * sd) / (math.cos(math.radians(CONE_DEC)) * cd)
    if x >= 1.0:
        return 0.0
    if x <= -1.0:
        return 180.0
    return math.degrees(math.acos(x))


def band_bounds(k):
    dec_lo = DEC_START + k * DEC_BAND
    dec_hi = dec_lo + DEC_BAND
    dec_near = min(max(CONE_DEC, dec_lo), dec_hi - 1e-9)
    w = circle_ra_halfwidth(dec_near) + RA_MARGIN
    return dec_lo, dec_hi, CONE_RA - w, CONE_RA + w


def build_box_query(dec_lo, dec_hi, ra_lo, ra_hi, gmax):
    return (f"SELECT {COLUMNS} FROM gaiadr3.gaia_source WHERE "
            f"CONTAINS(POINT('ICRS', ra, dec), "
            f"CIRCLE('ICRS', {CONE_RA}, {CONE_DEC}, {CONE_R})) = 1 "
            f"AND dec >= {FMT.format(dec_lo)} AND dec < {FMT.format(dec_hi)} "
            f"AND ra >= {FMT.format(ra_lo)} AND ra < {FMT.format(ra_hi)} "
            f"AND phot_g_mean_mag < {gmax}")


def verify_mirror_count(path, box, gmax, rows_plus_skipped):
    """Non-ESA mirror guard: async COUNT(*) for the region must match what the
    mirror returned (a foreign mirror could cap BELOW our canary silently)."""
    import tap_count
    name = f"verify_box_{path}"
    q = build_box_query(*box, gmax).replace(
        f"SELECT {COLUMNS}", "SELECT COUNT(*)", 1)
    tap_count.submit(name, q)
    val = tap_count.poll(name, wait_s=900)
    if val is None:
        raise RuntimeError(f"[{path}] mirror-verify COUNT job did not complete")
    if val != rows_plus_skipped:
        raise RuntimeError(
            f"[{path}] MIRROR MISMATCH: fetched {rows_plus_skipped} "
            f"(mirror != esa) but archive COUNT = {val}; refusing to bake")
    log(f"[{path}] non-ESA mirror verified by async COUNT = {val}")


def fetch_region(path, dec_lo, dec_hi, ra_lo, ra_hi, gmax, manifest, deadline):
    """Fetch one box (recursively subdividing at the cap). Returns (rec, rows,
    skipped, entry). Leaf results land in etl/build/box_<path>.bin + manifest."""
    key = f"box_{path}"
    binp = os.path.join(BAKE, f"{key}.bin")
    reg = manifest["regions"].get(key)
    if reg and os.path.exists(binp):
        with open(binp, "rb") as fh:
            rec = fh.read()
        return rec, reg["rows"], reg["skipped"], reg

    if deadline and time.time() > deadline:
        raise Deadline()

    box = (dec_lo, dec_hi, ra_lo, ra_hi)
    t0 = time.time()
    mirror, raw = tap_sync(None, build_box_query(*box, gmax))
    raw_path = os.path.join(RAW, f"{key}.csv")
    with open(raw_path, "wb") as fh:
        fh.write(raw)
    n_lines = raw.count(b"\n") - 1
    if n_lines > 400_000:
        raise RuntimeError(f"[{key}] {n_lines} rows in one box — query bug; aborting")
    raw_sha = sha256_file(raw_path)
    os.remove(raw_path)  # §8.4.1 rule 1

    if n_lines >= CAP_CANARY:
        raw = None
        if len(path) - 3 >= MAX_DEPTH:
            raise RuntimeError(f"[{key}] still capped at depth {len(path) - 3}; aborting")
        if (ra_hi - ra_lo) >= (dec_hi - dec_lo):
            mid = (ra_lo + ra_hi) / 2.0
            kids = [(path + "a", dec_lo, dec_hi, ra_lo, mid),
                    (path + "b", dec_lo, dec_hi, mid, ra_hi)]
            axis, midv = "ra", mid
        else:
            mid = (dec_lo + dec_hi) / 2.0
            kids = [(path + "a", dec_lo, mid, ra_lo, ra_hi),
                    (path + "b", mid, dec_hi, ra_lo, ra_hi)]
            axis, midv = "dec", mid
        log(f"[{key}] CAPPED at {n_lines} (mirror={mirror}) -> split {axis} at "
            f"{FMT.format(midv)} into {kids[0][0]}/{kids[1][0]}")
        recs, rows, skipped, kdetail = [], 0, 0, []
        for kp, dl, dh, rl, rh in kids:  # ascending order = concat order
            rc, rn, rsk, rentry = fetch_region(kp, dl, dh, rl, rh, gmax,
                                               manifest, deadline)
            recs.append(rc)
            rows += rn
            skipped += rsk
            kdetail.append({"key": f"box_{kp}", "rows": rn, "skipped": rsk})
            del rc
        rec = b"".join(recs)
        del recs
        entry = {"path": path, "box": [dec_lo, dec_hi, ra_lo, ra_hi],
                 "mirror": f"subdivided({mirror})", "axis": axis, "mid": midv,
                 "rows": rows, "skipped": skipped, "raw_sha256": None,
                 "capped_rows": n_lines, "capped_raw_sha256": raw_sha,
                 "sub_partitioned": True, "children": [k[0] for k in kids],
                 "children_detail": kdetail,
                 "seconds": round(time.time() - t0, 1),
                 "fetched_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                 "truncated_at_cap": False}
    else:
        rec, rows, skipped = quantise_csv(raw)
        raw = None
        entry = {"path": path, "box": [dec_lo, dec_hi, ra_lo, ra_hi],
                 "mirror": mirror, "rows": rows, "skipped": skipped,
                 "raw_sha256": raw_sha, "sub_partitioned": False,
                 "seconds": round(time.time() - t0, 1),
                 "fetched_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                 "truncated_at_cap": False}
        if mirror != "esa":
            verify_mirror_count(path, box, gmax, rows + skipped)

    assert len(rec) == entry["rows"] * 32, f"[{key}] rows*32 != bytes"
    entry["baked_bytes"] = len(rec)
    entry["baked_sha256"] = hashlib.sha256(rec).hexdigest()
    with open(binp, "wb") as fh:
        fh.write(rec)
    manifest["regions"][key] = entry
    save_manifest(manifest)
    log(f"[{key}] {entry['mirror']} rows={rows} skipped={skipped} "
        f"baked={entry['baked_bytes']}B raw_deleted dt={entry['seconds']}s")
    return rec, rows, skipped, entry


class Deadline(Exception):
    pass


def save_manifest(manifest):
    tmp = MANIFEST + ".tmp"
    with open(tmp, "w") as fh:
        json.dump(manifest, fh, indent=1)
    os.replace(tmp, MANIFEST)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gmax", type=float, default=20.0)
    ap.add_argument("--bands", type=str, default="",
                    help="comma list of band indices (default: all)")
    ap.add_argument("--min-free-gb", type=float, default=2.0)
    ap.add_argument("--deadline", type=float, default=0.0,
                    help="unix time after which to exit cleanly between regions "
                         "(chunked-run support); 0 = no deadline")
    ap.add_argument("--dry-run", action="store_true",
                    help="print the planned bands and exit")
    args = ap.parse_args()

    os.makedirs(RAW, exist_ok=True)
    os.makedirs(BAKE, exist_ok=True)
    os.makedirs(REPORTS, exist_ok=True)
    free = free_gb(BAKE)
    if free < args.min_free_gb:
        log(f"REFUSE: only {free:.1f} GB free at {BAKE}; gate {args.min_free_gb}")
        sys.exit(2)

    n_bands = int(round((DEC_END - DEC_START) / DEC_BAND))
    wanted = ({int(x) for x in args.bands.split(",")} if args.bands
              else set(range(n_bands)))

    if args.dry_run:
        for k in sorted(wanted):
            dl, dh, rl, rh = band_bounds(k)
            if circle_ra_halfwidth(min(max(CONE_DEC, dl), dh - 1e-9)) <= 0.0:
                continue
            log(f"band {k:02d}: dec [{FMT.format(dl)}, {FMT.format(dh)}) "
                f"ra [{FMT.format(rl)}, {FMT.format(rh)}) "
                f"w={circle_ra_halfwidth(min(max(CONE_DEC, dl), dh - 1e-9)):.3f} deg")
        return

    if os.path.exists(MANIFEST):
        manifest = json.load(open(MANIFEST))
        if manifest.get("scheme") != "dec-ra-box-exact-tiling-v1":
            raise RuntimeError("fetch_manifest.json exists with a different scheme; "
                               "archive it first (append-only provenance)")
    else:
        manifest = {"scheme": "dec-ra-box-exact-tiling-v1",
                    "cone": {"ra": CONE_RA, "dec": CONE_DEC, "r_deg": CONE_R,
                             "gmax": args.gmax},
                    "regions": {}}
    target = int(open(COUNT_FILE).read().strip()) if os.path.exists(COUNT_FILE) else None

    hit_deadline = False
    for k in sorted(wanted):
        dl, dh, rl, rh = band_bounds(k)
        if circle_ra_halfwidth(min(max(CONE_DEC, dl), dh - 1e-9)) <= 0.0:
            continue  # band misses the cone entirely
        try:
            fetch_region(f"d{k:02d}", dl, dh, rl, rh, args.gmax, manifest,
                         args.deadline or None)
        except Deadline:
            hit_deadline = True
            log(f"DEADLINE reached at band {k} — exiting cleanly (resumable)")
            break

    leaves = [r for r in manifest["regions"].values() if not r["sub_partitioned"]]
    tot_rows = sum(r["rows"] for r in leaves)
    tot_skip = sum(r["skipped"] for r in leaves)
    log(f"MANIFEST regions={len(manifest['regions'])} leaves={len(leaves)} "
        f"rows={tot_rows:,} skipped={tot_skip:,} fetched={tot_rows + tot_skip:,}")
    if target is not None:
        gate = "PASS" if tot_rows + tot_skip == target else "NOT-YET/FAIL"
        log(f"GATE sum(rows+skipped) == {target:,}: {gate} "
            f"(delta {tot_rows + tot_skip - target:+,})")
    if hit_deadline:
        sys.exit(0)
    if not wanted.symmetric_difference(set(range(n_bands))):
        # full run finished: stamp the integrity note
        manifest["gate"] = {"target_fetched_rows": target,
                            "fetched_rows_plus_skipped": tot_rows + tot_skip,
                            "pass": bool(target == tot_rows + tot_skip)}
        manifest["integrity_note"] = (
            "s006: exact dec/ra box tiling (half-open intervals + the same "
            "CONTAINS(CIRCLE) predicate as the DAY0 count) — every cone row "
            "falls in exactly one leaf box, so the sum equals the cone COUNT "
            "by construction. Supersedes the pixel-polygon scheme whose chord "
            "polygons could not tile exactly (measured: n32_4702 parent 60,792 "
            "vs children sum 60,789, complement 82, overhang 79).")
        save_manifest(manifest)
        log("FETCH DONE" + (" — GATE PASS" if manifest["gate"]["pass"] else " — GATE FAIL"))
    else:
        log("PARTIAL RUN DONE (band subset)")


if __name__ == "__main__":
    main()
