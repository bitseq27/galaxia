#!/usr/bin/env python3
"""etl/tap_count.py — async TAP COUNT helper (session 006).

The ESA archive silently caps RESULT ROWS at 50k (s005 discovery), but COUNT(*)
returns one row — async jobs are the reliable way to get exact tallies
(DAY0 precedent: job b3a87cff-... for the 15,573,949 cone count).

Usage:
    python3 etl/tap_count.py submit NAME QUERY   -> jobid (persisted to s006_jobs.json)
    python3 etl/tap_count.py poll NAME           -> phase / value when COMPLETED
    python3 etl/tap_count.py sync NAME QUERY     -> submit+poll+print (blocking)

Restart-safe: job ids persist in etl/reports/s006_jobs.json; poll re-attaches.
"""
import gzip
import json
import os
import sys
import time
import urllib.parse
import urllib.request

BASE = "https://gea.esac.esa.int/tap-server/tap"
HERE = os.path.dirname(os.path.abspath(__file__))
JOBS = os.path.join(HERE, "reports", "s006_jobs.json")


def _get(url, timeout=120):
    r = urllib.request.urlopen(url, timeout=timeout)
    body = r.read()
    if body[:2] == b"\x1f\x8b" or r.headers.get("Content-Encoding") == "gzip":
        body = gzip.decompress(body)
    return body


def _load():
    return json.load(open(JOBS)) if os.path.exists(JOBS) else {}


def _save(j):
    json.dump(j, open(JOBS, "w"), indent=1)


def submit(name, query):
    jobs = _load()
    if name in jobs and jobs[name].get("jid"):
        return jobs[name]["jid"]
    data = urllib.parse.urlencode(
        {"REQUEST": "doQuery", "LANG": "ADQL", "FORMAT": "csv",
         "QUERY": query, "PHASE": "RUN"}).encode()
    req = urllib.request.Request(f"{BASE}/async", data=data)
    resp = urllib.request.urlopen(req, timeout=60)
    jid = resp.headers.get("Location", resp.geturl()).rstrip("/").split("/")[-1]
    jobs[name] = {"jid": jid, "query": query,
                  "submitted": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    _save(jobs)
    print(f"[{name}] submitted {jid}", flush=True)
    return jid


def _parse_count_csv(body):
    lines = body.decode(errors="replace").strip().splitlines()
    return int(lines[-1].strip().strip('"'))


def poll(name, wait_s=0):
    jobs = _load()
    jid = jobs[name]["jid"]
    t0 = time.time()
    while True:
        phase = _get(f"{BASE}/async/{jid}/phase", timeout=30).decode().strip()
        if phase in ("COMPLETED", "ERROR", "ABORTED") or time.time() - t0 > wait_s:
            break
        time.sleep(5)
    if phase == "COMPLETED":
        body = _get(f"{BASE}/async/{jid}/results/result", timeout=120)
        try:
            val = _parse_count_csv(body)
        except ValueError:
            # some results come back as VOTable despite FORMAT=csv
            txt = body.decode(errors="replace")
            import re
            m = re.search(r"<TD[^>]*>\s*(\d+)\s*</TD>", txt)
            val = int(m.group(1)) if m else None
        jobs[name]["value"] = val
        jobs[name]["phase"] = phase
        _save(jobs)
        print(f"[{name}] {phase} value={val}", flush=True)
        return val
    jobs[name]["phase"] = phase
    _save(jobs)
    print(f"[{name}] phase={phase}", flush=True)
    return None


if __name__ == "__main__":
    cmd, name = sys.argv[1], sys.argv[2]
    if cmd == "submit":
        submit(name, sys.argv[3])
    elif cmd == "poll":
        poll(name, wait_s=int(sys.argv[3]) if len(sys.argv) > 3 else 0)
    elif cmd == "sync":
        submit(name, sys.argv[3])
        poll(name, wait_s=int(sys.argv[4]) if len(sys.argv) > 4 else 600)
    else:
        sys.exit(f"unknown cmd {cmd}")
