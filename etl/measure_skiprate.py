#!/usr/bin/env python3
"""etl/measure_skiprate.py — session 005, task item 5 (MEASURE, do not fix).

Skip-rate-vs-G curve of the no-parallax tail over the full Orion cone, from a
0.25% uniform sample (random_index MOD 400 = 0 — random_index is archive-wide
uniform and independent of magnitude/position), binned client side per 1-mag
G bin:
    total    sampled rows (x100 = cone estimate)
    noplx    parallax IS NULL OR <= 0  (the T0 skip class), split null/negative
    far_cap  0 < parallax <= 0.2 mas   (d >= 5 kpc — the fetcher's cap class)

WHY sampling: server-side GROUP BY needs ADQL expressions — ESA's parser
rejects GROUP BY FLOOR(...) ("Cannot parse query"; strict ADQL 2.0 groupBy =
column refs only) and ADQL has no CASE for conditional aggregation. 60 async
count jobs (20 bins x 3 classes) was the alternative; one sampled job with
client-side binning is cheaper on the archive and gives strictly more detail.
Sample size ~39k rows (kept under the archive 50k result cap — s005
discovery; a truncated sample is scan-order-biased): faint-bin rate
precision ~1%, sum x400 cross-checks the DAY0 count (15,573,949) within
sampling noise (~0.5%).

Async TAP (sync cannot scan the cone in its time budget — s001 precedent);
resumable: job id in etl/reports/skiprate_job.json, re-run re-attaches.

Output: etl/reports/skiprate_vs_g_s005.csv —
gbin,total_smp,noplx_smp,null_smp,neg_smp,far_cap_smp (x100 for cone scale).
The fix for the ~3.3 M-row tail defers to the target machine (Bailer-Jones
DR3 geometric distances etc.), per session 005 scope.
"""
import csv
import gzip
import json
import math
import os
import sys
import time
import urllib.parse
import urllib.request

BASE = "https://gea.esac.esa.int/tap-server/tap"
HERE = os.path.dirname(os.path.abspath(__file__))
REPORTS = os.path.join(HERE, "reports")
JOB_FILE = os.path.join(REPORTS, "skiprate_job.json")
OUT = os.path.join(REPORTS, "skiprate_vs_g_s005.csv")
SAMPLE_MOD = 400  # 0.25% ~ 39k rows — MUST stay under the archive's 50k result cap

QUERY = (
    "SELECT phot_g_mean_mag, parallax FROM gaiadr3.gaia_source "
    "WHERE CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', 83.82, -5.39, 20.0)) = 1 "
    f"AND phot_g_mean_mag < 20 AND MOD(random_index, {SAMPLE_MOD}) = 0"
)


def log(*a):
    print(*a, flush=True)


def get(url, timeout=300):
    body = urllib.request.urlopen(url, timeout=timeout).read()
    if body[:2] == b"\x1f\x8b":
        body = gzip.decompress(body)
    return body


def main():
    os.makedirs(REPORTS, exist_ok=True)
    jobs = json.load(open(JOB_FILE)) if os.path.exists(JOB_FILE) else {}
    jid = jobs.get("sample")
    if not jid:
        data = urllib.parse.urlencode(
            {"REQUEST": "doQuery", "LANG": "ADQL", "FORMAT": "csv",
             "QUERY": QUERY, "PHASE": "RUN"}).encode()
        resp = urllib.request.urlopen(
            urllib.request.Request(f"{BASE}/async", data=data), timeout=120)
        jid = resp.headers.get("Location", resp.geturl()).rstrip("/").split("/")[-1]
        jobs.update({"sample": jid, "query": QUERY, "sample_mod": SAMPLE_MOD,
                     "submitted": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})
        json.dump(jobs, open(JOB_FILE, "w"), indent=1)
        log("submitted async job", jid)

    deadline = time.time() + float(os.environ.get("SKIPRATE_POLL_S", "600"))
    while time.time() < deadline:
        phase = get(f"{BASE}/async/{jid}/phase", timeout=30).decode().strip()
        if phase in ("COMPLETED", "ERROR", "ABORTED"):
            break
        log("phase:", phase)
        time.sleep(20)
    log("phase:", phase)
    if phase != "COMPLETED":
        if phase in ("ERROR", "ABORTED"):
            log("error:", get(f"{BASE}/async/{jid}/error", timeout=30).decode()[:400])
            sys.exit(1)
        sys.exit(0)  # still running — re-run to re-attach

    body = get(f"{BASE}/async/{jid}/results/result", timeout=900).decode(errors="replace")
    lines = body.splitlines()
    # s005 discovery: the archive truncates results at 50k rows (sync AND
    # async) — a sample landing on the cap is BIASED (scan-order sliver).
    assert len(lines) - 1 < 49_999, \
        f"sample hit the 50k result cap ({len(lines)-1} rows) — increase SAMPLE_MOD" 
    if lines and lines[0].lstrip().startswith("<"):
        sys.exit("unexpected VOTable result — extend parser")
    header = [c.strip() for c in lines[0].split(",")]
    ig, ip = header.index("phot_g_mean_mag"), header.index("parallax")
    bins = {}
    n = 0
    for ln in lines[1:]:
        f = ln.split(",")
        try:
            g = float(f[ig])
        except (ValueError, IndexError):
            continue
        b = bins.setdefault(math.floor(g), dict(total=0, noplx=0, null=0, neg=0, far=0))
        b["total"] += 1
        plx = f[ip].strip() if ip < len(f) else ""
        if not plx:
            b["noplx"] += 1
            b["null"] += 1
        else:
            p = float(plx)
            if p <= 0:
                b["noplx"] += 1
                b["neg"] += 1
            elif p <= 0.2:
                b["far"] += 1
        n += 1
    with open(OUT, "w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["gbin", "total_smp", "noplx_smp", "null_smp", "neg_smp", "far_cap_smp"])
        for g in sorted(bins):
            b = bins[g]
            w.writerow([g, b["total"], b["noplx"], b["null"], b["neg"], b["far"]])
    jobs["finished"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    jobs["sampled_rows"] = n
    json.dump(jobs, open(JOB_FILE, "w"), indent=1)
    tot = sum(b["total"] for b in bins.values())
    nop = sum(b["noplx"] for b in bins.values())
    far = sum(b["far"] for b in bins.values())
    log(f"sampled rows={n} -> {OUT}")
    log(f"cone estimates (x{SAMPLE_MOD}): total={tot*SAMPLE_MOD:,} (DAY0 count 15,573,949) "
        f"noplx={nop*SAMPLE_MOD:,} far_cap={far*SAMPLE_MOD:,}")
    for g in sorted(bins):
        b = bins[g]
        r = b["noplx"] / max(b["total"], 1)
        log(f"  G {g:>2}-{g+1:<2} smp_total={b['total']:>6} noplx={b['noplx']:>6} "
            f"({r:6.2%}) null={b['null']:>6} neg={b['neg']:>5} far_cap={b['far']:>5}")


if __name__ == "__main__":
    main()
