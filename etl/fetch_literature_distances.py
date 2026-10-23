#!/usr/bin/env python3
"""etl/fetch_literature_distances.py — session 005, task step 2.

Resolve LITERATURE distances for the bright null/negative-parallax cone stars
(etl/reports/noplx_bright_g10_s005.csv, from fetch_noplx_bright.py) by position
cross-match <=1 arcsec. Provenance, never invention: every accepted distance
records source object id, parallax value/error/bibcode or distance-measurement
method/bibcode, match separation, and access date.

DEAD END (sandbox network, session 005 — do not re-litigate here):
  the planned ONE-SHOT VIZIER Hipparcos pull (I/239/hip_main via CDS TAP) is
  unreachable from this sandbox: tap.cds.unistra.fr does not resolve;
  cdsarc.cds.unistra.fr sits behind the Anubis JS anti-bot challenge;
  vizier.cds.unistra.fr / u-strasbg asu-tsv hang; iucaa/nao mirrors time out;
  nadc shows a login page; CADC path 404s; the Gaia archive has no hipparcos
  table. SIMBAD's REST (sim-coo) DOES work -> it is the primary source here
  (the handoff's sanctioned per-star fallback). SIMBAD plx_value is itself a
  literature compilation (for these bright stars, Hipparcos-derived).
  On the target machine, re-run with CDS TAP for the bulk HIP pull if desired.

Per-star resolution order (first hit wins, all recorded in `origin`):
  1. PLX_VALUE > 0 (SIMBAD adopted literature parallax) -> d = 1000/plx
  2. PLX:plx > 0 (single measurement entry with bibcode) -> d = 1000/plx
  3. Distance:distance in pc with method != 'paral' (photometric/spectroscopic
     literature distance, method + bibcode recorded) -> d = value
  4. unresolved -> written to etl/reports/literature_unresolved_s005.csv with
     the reason. Counted, never fabricated.

Match rule: SIMBAD row position (J2000) propagated with its own PMRA/PMDEC to
2016.0 (Gaia epoch), separation to the Gaia DR3 position <= 1.0 arcsec;
nearest wins. Query cone 1 arcmin (covers epoch drift), filtered locally.
SIMBAD radius note: sim-coo ignores RadiusUnit=arcsec (answers in arcmin).

Outputs:
  etl/norm/literature_distances.csv           source_id,ra,dec,d_pc,origin,accessed
  etl/reports/literature_unresolved_s005.csv  source_id,ra,dec,g_mag,reason
  etl/reports/simbad_cache/<source_id>.xml    raw responses (gitignored cache)
"""
import csv
import json
import math
import os
import sys
import time
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
NOPLX = os.path.join(HERE, "reports", "noplx_bright_g10_s005.csv")
OUT_TABLE = os.path.join(HERE, "norm", "literature_distances.csv")
OUT_UNRES = os.path.join(HERE, "reports", "literature_unresolved_s005.csv")
CACHE = os.path.join(HERE, "reports", "simbad_cache")
SIMBAD = "https://simbad.cds.unistra.fr/simbad/sim-coo"
MATCH_RADIUS_AS = 1.0
D_CAP_PC = 5000.0  # same cap the Gaia-parallax path enforces (fetch_orion.py)
EPOCH_GAIA = 2016.0
EPOCH_SIMBAD = 2000.0


def log(*a):
    print(*a, flush=True)


def simbad_votable(ra, dec, source_id, retries=3):
    os.makedirs(CACHE, exist_ok=True)
    p = os.path.join(CACHE, f"{source_id}.xml")
    if os.path.exists(p) and os.path.getsize(p) > 0:
        return open(p).read()
    url = SIMBAD + "?" + urllib.parse.urlencode({
        "Coord": f"{ra:.8f}{dec:+.8f}", "CoordFrame": "ICRS",
        "Radius": 1, "output.format": "VOTable"})
    last = None
    for attempt in range(retries):
        try:
            text = urllib.request.urlopen(url, timeout=90).read().decode(errors="replace")
            with open(p, "w") as fh:
                fh.write(text)
            return text
        except Exception as e:
            last = str(e)
            time.sleep(2 * (attempt + 1))
    raise RuntimeError(f"simbad query failed for {source_id}: {last}")


def parse_votable(text):
    """Rows of the results TABLE as dicts keyed by FIELD name.
    Namespace-agnostic: VOTable 1.2 tags arrive as {ns}TABLE etc."""
    def local(el):
        return el.tag.split("}")[-1]

    root = ET.fromstring(text)
    for table in (el for el in root.iter() if local(el) == "TABLE"):
        fields = [f.get("name") for f in table if local(f) == "FIELD"]
        if "MAIN_ID" not in fields:
            continue
        rows = []
        for tr in (el for el in table.iter() if local(el) == "TR"):
            tds = [(td.text or "") for td in tr if local(td) == "TD"]
            if len(tds) == len(fields):
                rows.append(dict(zip(fields, tds)))
        return rows
    return []


def sep_arcsec(ra1, dec1, ra2, dec2):
    d2r = math.pi / 180.0
    x = (ra1 - ra2) * d2r * math.cos((dec1 + dec2) / 2.0 * d2r)
    y = (dec1 - dec2) * d2r
    return math.hypot(x, y) / d2r * 3600.0


def f(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


def try_resolve(r, sep):
    """Distance tiers for ONE simbad row. Returns (d_pc, origin_suffix) or None.
    Tier quality rule: bare measurements (PLX:plx) need >=3 sigma when an error
    exists; SIMBAD's adopted PLX_VALUE is editorially selected -> accepted."""
    mid = r.get("MAIN_ID", "?").strip()
    otype = r.get("OTYPE_S", "").strip()
    plx_v = f(r.get("PLX_VALUE"))
    plx_m, plx_e = f(r.get("PLX:plx")), f(r.get("PLX:me"))
    plx_bib = r.get("PLX:bibcode", "").strip()
    dist_v, dist_u = f(r.get("Distance:distance")), r.get("Distance:unit", "").strip()
    dist_m = r.get("Distance:method", "").strip()
    dist_bib = r.get("Distance:bibcode", "").strip()
    base = (f"simbad:sim-coo id={mid} otype={otype} "
            f"match={sep:.2f}arcsec(pm-propagated)")
    if plx_v is not None and plx_v > 0:
        extra = f" PLX_VALUE={plx_v:g}mas"
        if plx_m:
            extra += (f" (meas {plx_m:g}" +
                      (f"+-{plx_e:g}" if plx_e else "") + f" {plx_bib})")
        return 1000.0 / plx_v, base + extra
    if plx_m is not None and plx_m > 0 and (plx_e in (None, 0) or plx_m / plx_e >= 3.0):
        return (1000.0 / plx_m,
                base + f" plx={plx_m:g}" +
                (f"+-{plx_e:g}" if plx_e else "") + f"mas {plx_bib}")
    if dist_v is not None and dist_v > 0 and dist_u.lower() == "pc" and dist_m != "paral":
        return dist_v, base + f" distance={dist_v:g}pc method={dist_m} {dist_bib}"
    return None


def resolve_star(st, accessed):
    """Return (d_pc, origin) or (None, reason).

    Multiple SIMBAD rows can sit within the match radius (system entries +
    per-component entries of multiples). Components share the system distance,
    so ANY row within <=1" (pm-propagated to 2016.0) may resolve it; pick the
    NEAREST row that yields a distance, record exactly which id was used."""
    ra, dec = float(st["ra"]), float(st["dec"])
    try:
        rows = parse_votable(simbad_votable(ra, dec, st["source_id"]))
    except Exception as e:
        return None, f"simbad query error: {e}"
    cands = []
    for r in rows:
        sra, sdec = f(r.get("RA_d")), f(r.get("DEC_d"))
        if sra is None or sdec is None:
            continue
        pmra, pmdec = f(r.get("PMRA")) or 0.0, f(r.get("PMDEC")) or 0.0  # mas/yr
        dt = EPOCH_GAIA - EPOCH_SIMBAD
        pra = sra + (pmra * dt) / 3.6e6 / max(math.cos(math.radians(sdec)), 1e-3)
        pdec = sdec + (pmdec * dt) / 3.6e6
        s = sep_arcsec(ra, dec, pra % 360.0, pdec)
        if s <= MATCH_RADIUS_AS:
            cands.append((s, r))
    if not cands:
        return None, (f"simbad: no object within {MATCH_RADIUS_AS}\" at 2016.0 "
                      f"({len(rows)} rows in 1' cone)")
    cands.sort(key=lambda c: c[0])
    for s, r in cands:
        hit = try_resolve(r, s)
        if hit:
            return hit
    n0 = cands[0]
    return None, (f"simbad: {len(cands)} object(s) within {MATCH_RADIUS_AS}\" "
                  f"(nearest id={n0[1].get('MAIN_ID', '?').strip()} "
                  f"sep={n0[0]:.2f}\") but none has plx>0 (or >=3 sigma) "
                  f"nor a non-paral distance measure")


def main():
    stars = list(csv.DictReader(open(NOPLX)))
    log(f"no-plx bright stars: {len(stars)}")
    accessed = time.strftime("%Y-%m-%d", time.gmtime())
    resolved, unresolved = [], []
    for i, st in enumerate(stars):
        d, origin = resolve_star(st, accessed)
        g = float(st["phot_g_mean_mag"])
        if d is not None:
            if d > D_CAP_PC:
                unresolved.append((st["source_id"], st["ra"], st["dec"], g,
                                   f"distance {d:.0f} pc > {D_CAP_PC:.0f} pc cap | {origin}"))
            else:
                resolved.append((st["source_id"], st["ra"], st["dec"],
                                 round(d, 3), origin, accessed))
        else:
            unresolved.append((st["source_id"], st["ra"], st["dec"], g, origin))
        if (i + 1) % 25 == 0:
            log(f"  {i + 1}/{len(stars)} resolved={len(resolved)}")
        time.sleep(0.3)

    os.makedirs(os.path.dirname(OUT_TABLE), exist_ok=True)
    with open(OUT_TABLE, "w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["source_id", "ra", "dec", "d_pc", "origin", "accessed"])
        w.writerows(sorted(resolved, key=lambda r: float(r[1])))
    with open(OUT_UNRES, "w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["source_id", "ra", "dec", "g_mag", "reason"])
        w.writerows(unresolved)
    log(json.dumps({"input": len(stars), "resolved": len(resolved),
                    "unresolved": len(unresolved)}))
    log(f"wrote {OUT_TABLE} and {OUT_UNRES}")
    by_sid = {r[0]: r for r in resolved}
    uns_sid = {u[0]: u for u in unresolved}
    for st in stars[:6]:
        r = by_sid.get(st["source_id"])
        if r:
            log(f"  G={float(st['phot_g_mean_mag']):.2f} sid={r[0]} d={r[3]} pc  {r[4][:110]}")
        else:
            u = uns_sid.get(st["source_id"])
            log(f"  G={float(st['phot_g_mean_mag']):.2f} sid={st['source_id']} UNRESOLVED: {u[4][:110] if u else '?'}")


if __name__ == "__main__":
    sys.exit(main())
