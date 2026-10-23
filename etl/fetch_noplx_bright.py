#!/usr/bin/env python3
"""etl/fetch_noplx_bright.py — session 005, task step 1.

Re-derive the list of the brightest cone stars with null/negative Gaia DR3
parallax (s001 found ~190 at G<10). Sync TAP, cone(83.82,-5.39,20 deg), G<10:

    SELECT ... FROM gaiadr3.gaia_source
    WHERE in cone AND phot_g_mean_mag < 10 AND (parallax IS NULL OR parallax <= 0)

Output: etl/reports/noplx_bright_g10_s005.csv (committed — provenance for the
literature cross-match step). Never invents data: this is the raw query result.
"""
import gzip
import json
import os
import time
import urllib.parse
import urllib.request

MIRRORS = [
    ("esa", "https://gea.esac.esa.int/tap-server/tap"),
    ("ari", "https://gaia.ari.uni-heidelberg.de/tap"),
    ("cds", "https://tap.cds.unistra.fr/tap"),
]
CONE_RA, CONE_DEC, CONE_R = 83.82, -5.39, 20.0
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "reports", "noplx_bright_g10_s005.csv")

QUERY = (
    "SELECT source_id, ra, dec, parallax, parallax_error, phot_g_mean_mag, bp_rp, "
    "phot_variable_flag "
    "FROM gaiadr3.gaia_source "
    "WHERE 1=CONTAINS(POINT('ICRS', ra, dec), "
    f"CIRCLE('ICRS', {CONE_RA}, {CONE_DEC}, {CONE_R})) "
    "AND phot_g_mean_mag < 10.0 "
    "AND (parallax IS NULL OR parallax <= 0) "
    "ORDER BY phot_g_mean_mag"
)


def tap_sync(query):
    last = None
    for name, base in MIRRORS:
        for attempt in range(3):
            try:
                data = urllib.parse.urlencode(
                    {"REQUEST": "doQuery", "LANG": "ADQL", "FORMAT": "csv",
                     "QUERY": query}).encode()
                body = urllib.request.urlopen(
                    urllib.request.Request(base + "/sync", data=data),
                    timeout=600).read()
                if body[:2] == b"\x1f\x8b":
                    body = gzip.decompress(body)
                return name, body.decode()
            except Exception as e:
                last = f"{name}: {e}"
                time.sleep(2 * (attempt + 1))
    raise RuntimeError(f"all mirrors failed: {last}")


def main():
    mirror, text = tap_sync(QUERY)
    lines = text.splitlines()
    n = len(lines) - 1
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as fh:
        fh.write(text if text.endswith("\n") else text + "\n")
    meta = {"mirror": mirror, "rows": n, "query": QUERY,
            "accessed": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    print(json.dumps(meta))
    print(f"saved {n} rows -> {OUT}")
    # brightest few, for the log
    for ln in lines[:8]:
        print(ln)


if __name__ == "__main__":
    main()
