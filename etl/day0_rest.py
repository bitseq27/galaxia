"""Fetch DAY0 results via plain TAP REST (no astroquery).

1. Reads the COMPLETED count job result -> etl/reports/day0_orion_count.txt
2. Submits the bright-subset job (TOP 20000, G<10) if absent, waits, saves
   -> etl/norm/orion_bright_g10_20k.csv
Restarts are safe: every step checks for an existing artifact/jobid first.
"""
import json, os, time, urllib.parse, urllib.request

BASE = "https://gea.esac.esa.int/tap-server/tap"
HERE = os.path.dirname(os.path.abspath(__file__))
REPORTS = os.path.join(HERE, "reports")
NORM = os.path.join(HERE, "norm")
JOBS_FILE = os.path.join(REPORTS, "day0_jobs.json")
COUNT_OUT = os.path.join(REPORTS, "day0_orion_count.txt")
BRIGHT_OUT = os.path.join(NORM, "orion_bright_g10_20k.csv")

Q_BRIGHT = (
    "SELECT TOP 20000 source_id, ra, dec, phot_g_mean_mag, bp_rp, parallax, "
    "pmra, pmdec, radial_velocity FROM gaiadr3.gaia_source "
    "WHERE CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', 83.82, -5.39, 20.0)) = 1 "
    "AND phot_g_mean_mag < 10.0 ORDER BY phot_g_mean_mag"
)

jobs = json.load(open(JOBS_FILE)) if os.path.exists(JOBS_FILE) else {}


def get(url, timeout=120):
    import gzip
    r = urllib.request.urlopen(url, timeout=timeout)
    body = r.read()
    if body[:2] == b"\x1f\x8b" or r.headers.get("Content-Encoding") == "gzip":
        body = gzip.decompress(body)
    return body


def post(url, fields, timeout=120):
    data = urllib.parse.urlencode(fields).encode()
    req = urllib.request.Request(url, data=data)
    resp = urllib.request.urlopen(req, timeout=timeout)
    return resp


# ── 1. count result ──────────────────────────────────────────────────────────
if not os.path.exists(COUNT_OUT):
    jid = jobs.get("count")
    assert jid, "no count jobid recorded"
    phase = get(f"{BASE}/async/{jid}/phase", timeout=30).decode().strip()
    print("count phase:", phase, flush=True)
    if phase == "COMPLETED":
        import io
        from astropy.io.votable import parse_single_table
        body = get(f"{BASE}/async/{jid}/results/result", timeout=120)
        # astroquery-submitted jobs return VOTable regardless of FORMAT param
        t = parse_single_table(io.BytesIO(body))
        n = str(int(t.array[0][0]))
        open(COUNT_OUT, "w").write(n)
        print("ORION COUNT G<20 =", n, flush=True)

# ── 2. bright subset ─────────────────────────────────────────────────────────
if not os.path.exists(BRIGHT_OUT):
    jid = jobs.get("bright")
    if not jid:
        resp = post(
            f"{BASE}/async",
            {"REQUEST": "doQuery", "LANG": "ADQL", "FORMAT": "csv",
             "QUERY": Q_BRIGHT, "PHASE": "RUN"},
        )
        # 303 redirect Location header carries the job URL
        jid = resp.headers.get("Location", resp.geturl()).rstrip("/").split("/")[-1]
        jobs["bright"] = jid
        json.dump(jobs, open(JOBS_FILE, "w"), indent=1)
        print("bright submitted:", jid, flush=True)
    t0 = time.time()
    while time.time() - t0 < 60 * 60:
        phase = get(f"{BASE}/async/{jid}/phase", timeout=30).decode().strip()
        if phase in ("COMPLETED", "ERROR", "ABORTED"):
            break
        time.sleep(10)
    print("bright phase:", phase, f"({time.time()-t0:.0f}s)", flush=True)
    if phase == "COMPLETED":
        csv = get(f"{BASE}/async/{jid}/results/result", timeout=600)
        open(BRIGHT_OUT, "wb").write(csv)
        n = csv.decode().count("\n") - 1
        print(f"bright rows={n} -> {BRIGHT_OUT}", flush=True)
    else:
        err = get(f"{BASE}/async/{jid}/error", timeout=30).decode()[:500]
        print("bright ERROR:", err, flush=True)
print("DAY0_REST DONE", flush=True)
