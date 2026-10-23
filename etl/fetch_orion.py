#!/usr/bin/env python3
"""etl/fetch_orion.py — DAY 0 streaming-partitioned Orion downloader (PLAN §8.4.1, §16.2).

Usage:
    python3 etl/fetch_orion.py --nside 32 --gmax 20.0 --workers 2 --max-partitions 5
    python3 etl/fetch_orion.py --slice 0,1,2,3,4        # explicit partition ids

Rules this enforces (PLAN §8.4.1):
  1. free-disk gate: refuses to start below --min-free-gb (default 40 on the target;
     override for the sandbox smoke slice with --min-free-gb 1)
  2. per-partition: fetch raw CSV -> quantise to 32 B/star -> DELETE raw immediately
  3. resumable: partitions whose baked output + manifest entry exist are skipped
  4. retries with backoff + automatic mirror failover (ESA -> ARI -> CDS -> NADC)
  5. provenance: etl/reports/fetch_manifest.json records endpoint, query, timestamp,
     row count, sha256 of raw and baked, for lineage.py (DAY0 §6)

Partitioning: HEALPix Nside=32 (RING), pixels intersecting the cone
(83.82, -5.39, r=20 deg). Each partition queries with the pixel POLYGON
(interpolated edges) so rows are not double-counted across partitions.

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

COLUMNS = ("source_id, ra, dec, parallax, phot_g_mean_mag, bp_rp")


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

    # Sequential by design at --workers 1-2 against ESA; the async job pool is
    # It.1 work. Sequential keeps throttling risk ~zero for the smoke slice.
    for pix in pixels:
        key = f"n{args.nside}_{pix}"
        baked = os.path.join(BAKE, f"{key}.bin")
        if key in manifest["partitions"] and os.path.exists(baked):
            log(f"[{key}] already baked, skipping")
            continue
        verts = pixel_polygon(args.nside, pix)
        q = (f"SELECT {COLUMNS} FROM gaiadr3.gaia_source WHERE "
             f"CONTAINS(POINT('ICRS', ra, dec), {adql_polygon(verts)}) = 1 "
             f"AND CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', {CONE_RA}, {CONE_DEC}, {CONE_R})) = 1 "
             f"AND phot_g_mean_mag < {args.gmax}")
        t0 = time.time()
        mirror, raw = tap_sync(None, q)
        raw_path = os.path.join(RAW, f"{key}.csv")
        with open(raw_path, "wb") as fh:
            fh.write(raw)
        n_lines = raw.count(b"\n") - 1
        if n_lines > 400_000:
            raise RuntimeError(
                f"[{key}] {n_lines} rows for one nside=32 pixel — polygon winding "
                f"likely inverted (expected ~70-90k); aborting before baking")
        rec, n, skipped = quantise_csv(raw)
        with open(baked, "wb") as fh:
            fh.write(rec)
        raw_sha = sha256_file(raw_path)
        os.remove(raw_path)  # §8.4.1 rule 1: delete raw immediately after baking
        manifest["partitions"][key] = {
            "pixel": pix, "nside": args.nside, "mirror": mirror,
            "rows": n, "skipped": skipped, "baked_bytes": len(rec),
            "raw_sha256": raw_sha, "baked_sha256": hashlib.sha256(rec).hexdigest(),
            "fetched_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "seconds": round(time.time() - t0, 1),
        }
        json.dump(manifest, open(MANIFEST, "w"), indent=1)
        log(f"[{key}] {mirror} rows={n} skipped={skipped} baked={len(rec)}B "
            f"raw_deleted dt={time.time()-t0:.0f}s")
    tot = sum(p["rows"] for p in manifest["partitions"].values())
    log(f"MANIFEST partitions={len(manifest['partitions'])} total_rows={tot}")
    log("FETCH DONE")


if __name__ == "__main__":
    main()
