#!/usr/bin/env python3
"""etl/fetch_orion.py — DAY 0 streaming-partitioned Orion downloader (PLAN §8.4.1, §16.2).

Usage:
    python3 etl/fetch_orion.py --nside 32 --gmax 20.0 --workers 2 --max-partitions 5
    python3 etl/fetch_orion.py --slice 0,1,2,3,4        # explicit partition ids

Rules this enforces (PLAN §8.4.1):
  1. free-disk gate: refuses to start below --min-free-gb (default 40 on the target;
     override for the sandbox smoke slice with --min-free-gb 1)
  2. per-partition: fetch raw CSV -> quantise to 32 B/star -> DELETE raw immediately
  3. resumable: partitions whose baked output + manifest entry exist are skipped;
     s006: child fetches are cached too (etl/build/children/ + children_manifest.json)
  4. retries with backoff + automatic mirror failover (ESA -> ARI -> CDS -> NADC)
  5. provenance: etl/reports/fetch_manifest.json records endpoint, query, timestamp,
     row count, sha256 of raw and baked, for lineage.py (DAY0 §6)
  6. s006 (the s005 50k-cap fix): a result at the >=49,999 canary is TRUNCATED by
     the archive — the pixel is automatically re-fetched as its 4 nside*2 children
     (recursively, floor nside 256) and the child records are CONCATENATED in
     ascending child-pixel-id order into the parent bin (rows*32 == bytes exact)

Partitioning: HEALPix Nside=32 (RING), pixels intersecting the cone
(83.82, -5.39, r=20 deg). Each partition queries with the pixel POLYGON
(interpolated edges) so rows are not double-counted across partitions.
Child pixels: NESTED 4p..4p+3 converted to RING (ring2nest/nest2ring) —
containment verified against healpy before first use (s006).

Quantised record v0 (32 B — final §8.4 schema lands in It.1):
    x,y,z  f32 pc (Sun-centred ICRS, It.0 shortcut frame)
    mag    f32 (Gaia G)
    r,g,b  f32 linear (crude bp_rp chain; Harre-Heller in It.2)
    flags  u32 (bit0 = provenance: 0=gaia_dr3_real, 1=synthetic; bit1 = has parallax)
"""
import argparse, gzip, hashlib, io, json, os, struct, sys, time
import urllib.parse, urllib.request

MIRRORS = [
    ("esa", "https://gea.esac.esa.int/tap-server/tap"),
    ("ari", "https://gaia.ari.uni-heidelberg.de/tap"),
    ("cds", "https://tap.cds.unistra.fr/tap"),
    ("nadc", "https://nadc.china-vo.org/tap"),
]
CONE_RA, CONE_DEC, CONE_R = 83.82, -5.39, 20.0
HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, "raw", "orion")
BAKE = os.path.join(HERE, "build")
REPORTS = os.path.join(HERE, "reports")
MANIFEST = os.path.join(REPORTS, "fetch_manifest.json")
# s006: sub-partitioned re-fetch state (the 50k-cap fix). Child results are
# cached individually so chunked foreground runs resume at child granularity.
CHILD_DIR = os.path.join(BAKE, "children")
CHILD_MANIFEST = os.path.join(REPORTS, "children_manifest.json")

COLUMNS = ("source_id, ra, dec, parallax, phot_g_mean_mag, bp_rp")

# s005 discovery: the ESA archive silently caps TAP results at 50k rows
# (sync AND async; raising MAXREC is ignored). A result at the canary is
# INCOMPLETE -> s006 fix: re-fetch as 4 children at 2x nside and concatenate.
CAP_CANARY = 49_999
MAX_NSIDE = 256  # recursion floor 32 -> 64 -> 128 -> 256, then hard-fail


def log(*a):
    print(*a, flush=True)


def free_gb(path):
    st = os.statvfs(path)
    return st.f_bavail * st.f_frsize / 1e9


def sha256_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def fetch(url, data, timeout=600):
    req = urllib.request.Request(url, data=data)
    r = urllib.request.urlopen(req, timeout=timeout)
    body = r.read()
    if body[:2] == b"\x1f\x8b":
        body = gzip.decompress(body)
    return body


def tap_sync(base, query, retries=3):
    last = None
    for attempt in range(retries):
        for name, url in MIRRORS:
            try:
                data = urllib.parse.urlencode(
                    {"REQUEST": "doQuery", "LANG": "ADQL", "FORMAT": "csv", "QUERY": query}
                ).encode()
                return name, fetch(f"{url}/sync", data)
            except Exception as e:  # throttle / 503 / timeout -> failover
                last = f"{name}: {e}"
                time.sleep(2 * (attempt + 1))
    raise RuntimeError(f"all mirrors failed: {last}")


def cone_pixels(nside):
    import healpy as hp
    import numpy as np
    vec = hp.ang2vec(CONE_RA, CONE_DEC, lonlat=True)
    pix = hp.query_disc(nside, vec, np.deg2rad(CONE_R), inclusive=True, nest=False)
    return [int(p) for p in pix]


def pixel_polygon(nside, pix, steps=4):
    """Pixel boundary as an ADQL POLYGON vertex list (deg), edges interpolated
    along great circles (healpy returns corners only)."""
    import healpy as hp
    import numpy as np
    corners = hp.boundaries(nside, pix, step=1).T  # (n_corners, 3)
    verts = []
    n = len(corners)
    for i in range(n):
        a, b = corners[i], corners[(i + 1) % n]
        for s in range(steps):
            t = s / steps
            v = a * (1 - t) + b * t
            v = v / np.linalg.norm(v)
            lat = float(np.degrees(np.arcsin(np.clip(v[2], -1, 1))))
            lon = float(np.degrees(np.arctan2(v[1], v[0])) % 360.0)
            verts.append((lon, lat))
    return verts


def adql_polygon(verts):
    pts = ",".join(f"{lon:.6f}, {lat:.6f}" for lon, lat in verts)
    return f"POLYGON('ICRS', {pts})"


def ring_children(nside, pix):
    """The 4 RING-ordered child pixel ids of RING pixel `pix` at `nside`.

    HEALPix's 4*p..4*p+3 parent/child relation holds ONLY in NESTED ordering —
    s006 verified the conversion (ring2nest -> x4 -> nest2ring) against healpy
    containment: centroids AND inward-pushed boundary corners of every child
    map back to the parent via ang2pix (18 sample parents incl. the densest
    truncated pixels, plus one level down n64->n128 for n32_4702)."""
    import healpy as hp
    q = int(hp.ring2nest(nside, pix))
    return sorted(int(c) for c in hp.nest2ring(2 * nside, [4 * q + k for k in range(4)]))


def build_query(nside, pix, gmax):
    verts = pixel_polygon(nside, pix)
    return (f"SELECT {COLUMNS} FROM gaiadr3.gaia_source WHERE "
            f"CONTAINS(POINT('ICRS', ra, dec), {adql_polygon(verts)}) = 1 "
            f"AND CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', {CONE_RA}, {CONE_DEC}, {CONE_R})) = 1 "
            f"AND phot_g_mean_mag < {gmax}")


def fetch_records(nside, pix, gmax, childman):
    """Fetch one pixel quantised to 32 B records, sub-partitioning on the cap.

    Returns (records, rows, skipped, info). A result of >= CAP_CANARY rows
    means the archive truncated it: re-fetch as the 4 children at 2*nside and
    CONCATENATE child records in ascending child-pixel-id order (s005
    next_action spec); recurse if a child is itself capped. Child results are
    cached (CHILD_DIR bin + CHILD_MANIFEST entry) for resumability. Raw CSVs
    are deleted immediately after use (§8.4.1 rule 1) at every level."""
    key = f"n{nside}_{pix}"
    cached_bin = os.path.join(CHILD_DIR, f"{key}.bin")
    ent = childman.get(key)
    if nside > 32 and ent and os.path.exists(cached_bin):
        with open(cached_bin, "rb") as fh:
            rec = fh.read()
        log(f"[{key}] cached child hit: rows={ent['rows']} skipped={ent['skipped']}")
        return rec, ent["rows"], ent["skipped"], ent

    t0 = time.time()
    mirror, raw = tap_sync(None, build_query(nside, pix, gmax))
    raw_path = os.path.join(RAW, f"{key}.csv")
    with open(raw_path, "wb") as fh:
        fh.write(raw)
    n_lines = raw.count(b"\n") - 1
    if n_lines > 400_000:
        raise RuntimeError(
            f"[{key}] {n_lines} rows for one pixel — polygon winding "
            f"likely inverted; aborting before baking")
    raw_sha = sha256_file(raw_path)
    os.remove(raw_path)  # §8.4.1 rule 1: delete raw immediately after use

    if n_lines >= CAP_CANARY:
        # The archive capped this result — it is INCOMPLETE. Sub-partition.
        if nside >= MAX_NSIDE:
            raise RuntimeError(
                f"[{key}] still capped at nside={nside} (>= {MAX_NSIDE}) — "
                f"polygon bug or archive change; refusing to bake")
        raw = None  # free before recursion (1 GB sandbox)
        kids = ring_children(nside, pix)
        log(f"[{key}] CAPPED at {n_lines} rows (mirror={mirror}) -> "
            f"sub-partitioning into nside={2 * nside} children {kids}")
        recs, rows, skipped, kdetail = [], 0, 0, []
        for c in kids:  # ascending child id = concatenation order (s005 spec)
            rc, rn, rsk, rinfo = fetch_records(2 * nside, c, gmax, childman)
            recs.append(rc)
            rows += rn
            skipped += rsk
            kd = {"key": rinfo["key"], "rows": rn, "skipped": rsk,
                  "mirror": rinfo["mirror"], "seconds": rinfo["seconds"]}
            if rinfo.get("sub_partitioned"):
                kd["sub_partitioned"] = True
                kd["children"] = rinfo["children"]
            kdetail.append(kd)
            del rc
        rec = b"".join(recs)
        del recs
        if rows + skipped <= n_lines:
            log(f"[{key}] WARNING: children total {rows + skipped} <= capped "
                f"parent {n_lines} — false-positive canary (true count exactly "
                f"at cap) or polygon problem; investigate before trusting")
        info = {"key": key, "pixel": pix, "nside": nside,
                "mirror": f"sub-partitioned({mirror})",
                "rows": rows, "skipped": skipped, "raw_sha256": None,
                "capped_rows": n_lines, "capped_raw_sha256": raw_sha,
                "sub_partitioned": True, "children": kids,
                "children_detail": kdetail,
                "seconds": round(time.time() - t0, 1),
                "fetched_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    else:
        rec, rows, skipped = quantise_csv(raw)
        raw = None
        info = {"key": key, "pixel": pix, "nside": nside, "mirror": mirror,
                "rows": rows, "skipped": skipped, "raw_sha256": raw_sha,
                "seconds": round(time.time() - t0, 1),
                "fetched_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}

    assert len(rec) == info["rows"] * 32, f"[{key}] rows*32 != bytes"
    info["baked_bytes"] = len(rec)
    info["baked_sha256"] = hashlib.sha256(rec).hexdigest()
    if nside > 32:
        os.makedirs(CHILD_DIR, exist_ok=True)
        with open(cached_bin, "wb") as fh:
            fh.write(rec)
        childman[key] = info
        json.dump(childman, open(CHILD_MANIFEST, "w"), indent=1)
    log(f"[{key}] {info['mirror']} rows={rows} skipped={skipped} "
        f"baked={len(rec)}B raw_deleted dt={info['seconds']}s")
    return rec, rows, skipped, info


def quantise_csv(raw_bytes):
    """CSV rows -> 32 B records. Returns (records bytes, n_rows, n_skipped)."""
    import numpy as np
    from galaxia_quant import record  # local helper, see below

    rows = []
    skipped = 0
    lines = raw_bytes.decode(errors="replace").splitlines()
    header = [c.strip() for c in lines[0].split(",")] if lines else []
    idx = {c: i for i, c in enumerate(header)}
    for line in lines[1:]:
        f = line.split(",")
        try:
            ra = float(f[idx["ra"]]); dec = float(f[idx["dec"]])
            g = float(f[idx["phot_g_mean_mag"]])
        except (ValueError, IndexError):
            skipped += 1
            continue
        plx = f[idx["parallax"]].strip()
        has_plx = bool(plx)
        d_pc = 1000.0 / float(plx) if has_plx and float(plx) > 0.05 else None
        if d_pc is None or d_pc > 5000.0:
            skipped += 1
            continue
        bprp = f[idx["bp_rp"]].strip()
        rows.append((ra, dec, d_pc, g, float(bprp) if bprp else float("nan"), has_plx))
    out = bytearray()
    for ra, dec, d, g, bprp, has_plx in rows:
        out += record(ra, dec, d, g, bprp, has_plx)
    return bytes(out), len(rows), skipped


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--nside", type=int, default=32)
    ap.add_argument("--gmax", type=float, default=20.0)
    ap.add_argument("--workers", type=int, default=2, help="ceiling 4-6; be a good citizen")
    ap.add_argument("--max-partitions", type=int, default=0, help="0 = all cone pixels")
    ap.add_argument("--slice", type=str, default="", help="comma list of pixel ids")
    ap.add_argument("--min-free-gb", type=float, default=40.0)
    args = ap.parse_args()

    os.makedirs(RAW, exist_ok=True)
    os.makedirs(BAKE, exist_ok=True)
    os.makedirs(REPORTS, exist_ok=True)

    free = free_gb(BAKE)
    if free < args.min_free_gb:
        log(f"REFUSE: only {free:.1f} GB free at {BAKE}; gate is {args.min_free_gb} GB (§8.4.1 rule 5)")
        sys.exit(2)
    log(f"free disk at {BAKE}: {free:.1f} GB (gate {args.min_free_gb})")

    if args.slice:
        pixels = [int(x) for x in args.slice.split(",")]
    else:
        pixels = cone_pixels(args.nside)
        if args.max_partitions:
            pixels = pixels[: args.max_partitions]
    log(f"partitions: {len(pixels)} (nside={args.nside})")

    manifest = json.load(open(MANIFEST)) if os.path.exists(MANIFEST) else {"partitions": {}}
    if manifest.get("scheme") != "pixel-polygon-v1" and manifest.get("scheme"):
        raise RuntimeError(
            "fetch_manifest.json belongs to a different scheme "
            f"({manifest.get('scheme')}) — this fetcher owns the pixel-polygon "
            "scheme; s006+ uses etl/fetch_cone_exact.py (box tiling)")
    manifest.setdefault("partitions", {})
    childman = json.load(open(CHILD_MANIFEST)) if os.path.exists(CHILD_MANIFEST) else {}

    # Sequential by design at --workers 1-2 against ESA; the async job pool is
    # It.1 work. Sequential keeps throttling risk ~zero for the smoke slice.
    for pix in pixels:
        key = f"n{args.nside}_{pix}"
        baked = os.path.join(BAKE, f"{key}.bin")
        if key in manifest["partitions"] and os.path.exists(baked):
            log(f"[{key}] already baked, skipping")
            continue
        # s006: fetch_records sub-partitions automatically when the archive
        # caps a result (>= 49,999 rows) — the s005 truncation fix.
        rec, n, skipped, info = fetch_records(args.nside, pix, args.gmax, childman)
        with open(baked, "wb") as fh:
            fh.write(rec)
        del rec
        entry = {
            "pixel": pix, "nside": args.nside, "mirror": info["mirror"],
            "rows": n, "skipped": skipped, "baked_bytes": info["baked_bytes"],
            "raw_sha256": info.get("raw_sha256"),
            "baked_sha256": info["baked_sha256"],
            "fetched_at": info["fetched_at"],
            "seconds": info["seconds"],
            "truncated_at_cap": False,  # s006: sub-partitioning guarantees completeness
        }
        if info.get("sub_partitioned"):
            entry["sub_partitioned"] = True
            entry["children"] = info["children"]
            entry["children_detail"] = info["children_detail"]
            entry["capped_rows"] = info["capped_rows"]
            entry["capped_raw_sha256"] = info["capped_raw_sha256"]
        manifest["partitions"][key] = entry
        json.dump(manifest, open(MANIFEST, "w"), indent=1)
        log(f"[{key}] rows={n} skipped={skipped} baked={info['baked_bytes']}B "
            f"dt={info['seconds']}s" + (" SUB-PARTITIONED" if entry.get("sub_partitioned") else ""))
    tot = sum(p["rows"] for p in manifest["partitions"].values())
    tot_fetched = sum(p["rows"] + p["skipped"] for p in manifest["partitions"].values())
    log(f"MANIFEST partitions={len(manifest['partitions'])} total_rows={tot} "
        f"total_fetched={tot_fetched}")
    log("FETCH DONE")


if __name__ == "__main__":
    main()
