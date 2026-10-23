#!/usr/bin/env python3
"""etl/bake_literature_bin.py — session 005, task step 3.

Quantise the resolved literature-distance stars (etl/norm/literature_distances.csv,
SIMBAD-compiled; see fetch_literature_distances.py) into the 32 B v0 record as an
EXTRA T0 partition bin: etl/build/n32_lit.bin. octree_build.py globs n32_*.bin,
so the next re-bake consumes it with no builder changes.

Photometry (G, bp_rp) is joined by source_id from the Gaia DR3 query that
produced the list (etl/reports/noplx_bright_g10_s005.csv) — positions and
magnitudes stay Gaia DR3 real data; ONLY the distance is literature.

flags = bit1 (no usable Gaia DR3 parallax) | bit2 (distance_from_literature) = 6;
bit0 stays 0 (real provenance). Verified after write by decoding the bin.

Outputs:
  etl/build/n32_lit.bin                  (gitignored artifact)
  etl/reports/literature_manifest.json   (committed provenance)
"""
import csv
import hashlib
import json
import math
import os
import struct
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from galaxia_quant import (FLAG_DIST_FROM_LITERATURE, RECORD_BYTES, record)

LIT = os.path.join(HERE, "norm", "literature_distances.csv")
NOPLX = os.path.join(HERE, "reports", "noplx_bright_g10_s005.csv")
OUT = os.path.join(HERE, "build", "n32_lit.bin")
MANIFEST = os.path.join(HERE, "reports", "literature_manifest.json")


def sha256_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    lit = list(csv.DictReader(open(LIT)))
    phot = {r["source_id"]: r for r in csv.DictReader(open(NOPLX))}
    out = bytearray()
    baked = []  # d_pc per baked record, in order (for the verify pass)
    n, missing_phot = 0, 0
    for r in lit:
        p = phot.get(r["source_id"])
        if p is None:
            missing_phot += 1
            continue
        bprp = p["bp_rp"].strip()
        out += record(float(r["ra"]), float(r["dec"]), float(r["d_pc"]),
                      float(p["phot_g_mean_mag"]),
                      float(bprp) if bprp else float("nan"),
                      has_parallax=False,
                      extra_flags=FLAG_DIST_FROM_LITERATURE)
        baked.append(float(r["d_pc"]))
        n += 1
    assert len(out) == n * RECORD_BYTES, "record size drift"
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "wb") as fh:
        fh.write(out)

    # decode-verify: flags, round-trip distance, finiteness
    worst_dd, bad_flags = 0.0, 0
    for i in range(n):
        x, y, z, mag, rr, gg, bb, flags = struct.unpack_from("<4f3fI", out, i * 32)
        if flags != (2 | FLAG_DIST_FROM_LITERATURE) or not all(
                math.isfinite(v) for v in (x, y, z, mag, rr, gg, bb)):
            bad_flags += 1
        d = math.sqrt(x * x + y * y + z * z)
        dd = abs(d - baked[i]) / baked[i]
        worst_dd = max(worst_dd, dd)
    assert bad_flags == 0, f"{bad_flags} records with wrong flags/non-finite"
    assert worst_dd < 1e-5, f"round-trip distance drift {worst_dd}"

    manifest = {
        "bin": "etl/build/n32_lit.bin",
        "rows": n, "bytes": len(out),
        "missing_photometry_rows": missing_phot,
        "flags_value": 2 | FLAG_DIST_FROM_LITERATURE,
        "flags_semantics": "bit0=0 real, bit1=no usable Gaia DR3 parallax, "
                           "bit2=distance_from_literature",
        "inputs": {
            "literature_distances.csv": {"sha256": sha256_file(LIT),
                                         "rows": len(lit)},
            "noplx_bright_g10_s005.csv": {"sha256": sha256_file(NOPLX),
                                          "rows": len(phot)},
        },
        "baked_sha256": hashlib.sha256(bytes(out)).hexdigest(),
        "roundtrip_max_rel_distance_error": worst_dd,
        "baked_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }
    json.dump(manifest, open(MANIFEST, "w"), indent=1)
    print(json.dumps({k: manifest[k] for k in
                      ("rows", "bytes", "missing_photometry_rows",
                       "roundtrip_max_rel_distance_error", "baked_sha256")}))
    print("LITERATURE BIN DONE")


if __name__ == "__main__":
    main()
