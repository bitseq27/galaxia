#!/usr/bin/env python3
"""etl/enrich_bright_csv.py — session 005, task step 4 (It.0 CSV path).

Add a `d_lit_pc` column to etl/norm/orion_bright_g10_20k.csv, filled ONLY for
source_ids resolved in etl/norm/literature_distances.csv (all of which have
null/negative Gaia DR3 parallax by construction — the loader keeps preferring
a real parallax whenever one exists; d_lit_pc only rescues rows it used to
skip). Provenance: the literature table's `origin` column carries the full
lineage per star; this script copies nothing but the distance.

Backward compatible: the Rust loader is header-driven and ignores unknown
columns; an unfilled d_lit_pc behaves exactly like before (row skipped).
"""
import csv
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
BRIGHT = os.path.join(HERE, "norm", "orion_bright_g10_20k.csv")
LIT = os.path.join(HERE, "norm", "literature_distances.csv")


def main():
    lit = {r["source_id"]: float(r["d_pc"])
           for r in csv.DictReader(open(LIT))}
    with open(BRIGHT, newline="") as fh:
        rows = list(csv.DictReader(fh))
        fields = list(rows[0].keys())
    if "d_lit_pc" in fields:
        sys.exit("d_lit_pc column already present — refusing to double-apply")
    fields.append("d_lit_pc")
    filled = usable_plx = 0
    for r in rows:
        d = lit.get(r["source_id"])
        if d is not None:
            # "usable parallax" == what the Rust loader accepts: plx > 0.2 mas
            # (d = 1000/plx < 5000 pc). The no-plx list admits NULL OR <= 0,
            # so negative-parallax rows legitimately land here; a row with a
            # USABLE positive parallax would mean the derived list is wrong.
            plx = r.get("parallax", "").strip()
            if plx and float(plx) > 0.2:
                usable_plx += 1
                continue
            r["d_lit_pc"] = repr(d)
            filled += 1
        else:
            r["d_lit_pc"] = ""
    assert usable_plx == 0, f"{usable_plx} literature ids had a usable Gaia parallax — list is wrong"
    with open(BRIGHT, "w", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=fields, lineterminator="\n")
        w.writeheader()
        w.writerows(rows)
    missing = len(lit) - filled
    print(f"rows={len(rows)} d_lit_pc filled={filled} lit_ids_not_in_csv={missing}")
    if missing:
        print("WARNING: resolved ids absent from the bright CSV:", missing)


if __name__ == "__main__":
    main()
