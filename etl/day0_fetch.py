"""DAY0 fetch (templates/DAY0-orion-fetch.md §2): Orion cone COUNT + bright subset.

Robust to interruptions: job ids are persisted to etl/reports/day0_jobs.json
immediately after launch, and existing server-side jobs are re-attached by id.

Usage: python3 etl/day0_fetch.py
"""
import json, os, time
from astroquery.gaia import Gaia

REPORTS = os.path.join(os.path.dirname(__file__), "reports")
NORM = os.path.join(os.path.dirname(__file__), "norm")
JOBS_FILE = os.path.join(REPORTS, "day0_jobs.json")
os.makedirs(REPORTS, exist_ok=True)
os.makedirs(NORM, exist_ok=True)

Q_COUNT = (
    "SELECT COUNT(*) AS n FROM gaiadr3.gaia_source "
    "WHERE CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', 83.82, -5.39, 20.0)) = 1 "
    "AND phot_g_mean_mag < 20.0"
)
Q_BRIGHT = (
    "SELECT TOP 20000 source_id, ra, dec, phot_g_mean_mag, bp_rp, parallax, "
    "pmra, pmdec, radial_velocity FROM gaiadr3.gaia_source "
    "WHERE CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', 83.82, -5.39, 20.0)) = 1 "
    "AND phot_g_mean_mag < 10.0 ORDER BY phot_g_mean_mag"
)

jobs = {}
if os.path.exists(JOBS_FILE):
    jobs = json.load(open(JOBS_FILE))

def ensure(name, query, known_id=None):
    if name in jobs:
        jid = jobs[name]
    elif known_id:
        jid = known_id
        jobs[name] = jid
    else:
        j = Gaia.launch_job_async(query, verbose=False)
        jid = j.jobid
        jobs[name] = jid
    json.dump(jobs, open(JOBS_FILE, "w"), indent=1)
    print(f"[{name}] jobid={jid}", flush=True)
    return jid

# Re-attach the count job launched before the interruption, if not yet recorded.
count_id = ensure("count", Q_COUNT, known_id="b3a87cff-ceb8-11f1-9d98-bc97e148b76b-O"
                  if "count" not in jobs else None)
bright_id = ensure("bright", Q_BRIGHT)

def wait(name, jid, max_min=90):
    out_count = os.path.join(REPORTS, "day0_orion_count.txt")
    out_bright = os.path.join(NORM, "orion_bright_g10_20k.csv")
    out = out_count if name == "count" else out_bright
    if os.path.exists(out) and os.path.getsize(out) > 0:
        print(f"[{name}] already have result at {out}", flush=True)
        return
    t0 = time.time()
    while time.time() - t0 < max_min * 60:
        j = Gaia.load_async_job(jobid=jid)
        if j.is_finished():
            res = j.get_results()
            if name == "count":
                n = int(res["n"][0])
                open(out_count, "w").write(str(n))
                print(f"[count] ORION CONE G<20 ROWS = {n}", flush=True)
            else:
                t = res.to_table()
                t.write(out_bright, format="csv", overwrite=True)
                print(f"[bright] rows={len(t)} -> {out_bright}", flush=True)
            return
        time.sleep(10)
    print(f"[{name}] TIMEOUT waiting for {jid}", flush=True)

wait("count", count_id)
wait("bright", bright_id)
print("DAY0_FETCH DONE", flush=True)
