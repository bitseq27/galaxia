#!/usr/bin/env python3
"""etl/octree_build.py — T0 octree bake (PLAN §H2, §16.4 It.1). MEMORY-FRUGAL.

Reads quantised partitions (etl/build/n32_*.bin, 32 B/star v0) via memmap,
Morton-sorts, cuts leaves at ~LEAF_CAP, emits:

    etl/build/t0/tree_meta.json | nodes.bin (80 B/node) | stars.bin | leaves.idx

Peak RAM kept under ~500 MB so the 1 GB sandbox can bake the full 8.6 M-star
cone: memmap input, sliding two-level leaf cut, single stable argsort, chunked
gather on write, sorted star array never materialised.

Node record v0 (80 B, matches galaxia-data NodeRecord):
    morton u64, depth u32, flags u32 (bit0 leaf), count u32, leaf_id u32,
    brightest f32, centre f32x3 (pc, Sun-centred ICRS), radius f32, pad u32,
    children i32x8 (node indices, -1 empty)

Deterministic: stable sorts, fixed tie-breaks, sha256 over stars.bin.
"""
import argparse, hashlib, json, os, sys
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "build")
OUT = os.path.join(SRC, "t0")
LEAF_CAP = 100_000
MAX_DEPTH = 16
REC = np.dtype([("x", "<f4"), ("y", "<f4"), ("z", "<f4"), ("mag", "<f4"),
                ("r", "<f4"), ("g", "<f4"), ("b", "<f4"), ("flags", "<u4")])
NODE = np.dtype([("morton", "<u8"), ("depth", "<u4"), ("flags", "<u4"),
                 ("count", "<u4"), ("leaf_id", "<u4"), ("brightest", "<f4"),
                 ("cx", "<f4"), ("cy", "<f4"), ("cz", "<f4"), ("radius", "<f4"),
                 ("pad", "<u4"), ("children", "<i4", (8,))])


def load_partitions_memmap(limit_parts=0):
    # s006: top-level *.bin = leaf partitions of whichever fetch scheme ran
    # (pixel n32_*.bin, literature n32_lit.bin, exact-tiling box_*.bin).
    # Subdirectories (internal/, children/) hold concatenated parents /
    # pixel-scheme child caches — excluded so stars are never double-counted.
    files = sorted(f for f in os.listdir(SRC)
                   if f.endswith(".bin") and not f.startswith(".")
                   and os.path.isfile(os.path.join(SRC, f)))
    if limit_parts:
        files = files[:limit_parts]
    sizes = [(f, os.path.getsize(os.path.join(SRC, f))) for f in files]
    sizes = [(f, sz) for f, sz in sizes if sz > 0]
    if not sizes:
        sys.exit("no partitions baked yet")
    total = sum(sz // 32 for _, sz in sizes)
    os.makedirs(OUT, exist_ok=True)
    out = np.memmap(os.path.join(OUT, ".stars.tmp"), dtype=REC, mode="w+", shape=(total,))
    off = 0
    for f, sz in sizes:
        part = np.fromfile(os.path.join(SRC, f), dtype=REC)
        out[off:off + len(part)] = part
        off += len(part)
        del part
    out.flush()
    return out, len(sizes)


def morton3(x, y, z, depth, chunk=1 << 20):
    # s006: chunked driver — elementwise math IDENTICAL to the whole-array
    # version, but the u64/bool temporaries are bounded (~35 MB per 1 M chunk
    # instead of ~90 MB each at full length). The whole-array version peaked
    # ~600 MB anon at 11.03 M stars and was OOM-killed in the 1 GB sandbox
    # (it fit at 8.58 M in s002-s005 — the s006 completeness fix grew T0 29%).
    code = np.zeros(len(x), dtype=np.uint64)
    for a in range(0, len(x), chunk):
        b = min(a + chunk, len(x))
        xa, ya, za = x[a:b], y[a:b], z[a:b]
        ca = np.zeros(b - a, dtype=np.uint64)
        for d in range(depth):
            bit = np.uint32(1) << np.uint32(d)
            bx = (xa & bit) != 0
            by = (ya & bit) != 0
            bz = (za & bit) != 0
            ca |= (bx.astype(np.uint64) << np.uint64(3 * d)) \
                | (by.astype(np.uint64) << np.uint64(3 * d + 1)) \
                | (bz.astype(np.uint64) << np.uint64(3 * d + 2))
            del bx, by, bz
        code[a:b] = ca
        del ca
    return code


def cell_geom(code, d, center, half):
    c = np.array(center, np.float64)
    s = 2 * half
    for bit in range(d):
        s /= 2
        sh = 3 * (d - 1 - bit)
        off = np.array([(code >> sh) & 1, (code >> (sh + 1)) & 1,
                        (code >> (sh + 2)) & 1], np.float64) - 0.5
        c = c + off * s
    return c, float(np.sqrt(3) * s / 2)


def log_mem(label):
    """RSS trace (s006): the 1 GB sandbox OOM-kills silently; the last printed
    phase before a kill identifies the allocating step. Cheap on any host."""
    try:
        with open("/proc/self/status") as fh:
            for line in fh:
                if line.startswith("VmRSS:"):
                    print(f"MEM {label}: {line.split()[1]} kB", flush=True)
                    break
    except OSError:
        pass


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--limit-parts", type=int, default=0)
    ap.add_argument("--leaf-cap", type=int, default=LEAF_CAP)
    args = ap.parse_args()
    os.makedirs(OUT, exist_ok=True)

    stars, nparts = load_partitions_memmap(args.limit_parts)
    n = len(stars)
    print(f"loaded {n} stars from {nparts} partitions", flush=True)
    log_mem("after load")

    # ── root cube + integer coords (f32 maths, int temps) ───────────────────
    xs, ys, zs = stars["x"], stars["y"], stars["z"]
    lo = np.array([xs.min(), ys.min(), zs.min()], np.float64)
    hi = np.array([xs.max(), ys.max(), zs.max()], np.float64)
    center = (lo + hi) / 2.0
    half = float(np.max(hi - center)) * 1.0001
    scale = np.float32((1 << MAX_DEPTH) / (2 * half))
    half_off = np.float32(0.5 * (1 << MAX_DEPTH))
    q = [None, None, None]
    for i, (v, c) in enumerate(((xs, center[0]), (ys, center[1]), (zs, center[2]))):
        t = ((v - np.float32(c)) * scale + half_off)
        q[i] = np.clip(t.astype(np.uint32), 0, (1 << MAX_DEPTH) - 1)
        del t
        log_mem(f"after q[{i}]")
    codes = morton3(q[0], q[1], q[2], MAX_DEPTH)
    log_mem("after morton3")
    del q, xs, ys, zs

    order = np.argsort(codes, kind="stable")
    log_mem("after argsort")
    order = order.astype(np.uint32)  # s006: n < 2^32 — halves an 88 MB array
    codes_s = codes[order]
    del codes
    log_mem("after codes_s gather")

    # ── leaf cutting: sliding two-level window (2 levels live at a time) ────
    # s006 memory fix: codes_s is SORTED, so pref (monotone right-shift) is
    # sorted — run-length-encode directly instead of np.unique, which copies
    # AND re-sorts the full 11 M-element array (+~450 MB peak at 11.03 M stars
    # — OOM-killed in the 1 GB sandbox; it fit at 8.58 M). Outputs identical:
    # u ascending u64, counts narrowed to int32 (n < 2^31).
    CH = 1 << 20

    def level(d):
        n_ = codes_s.size
        if d < MAX_DEPTH:
            sh = np.uint64(3 * (MAX_DEPTH - d))
            pref = np.empty_like(codes_s)
            for a in range(0, n_, CH):
                np.right_shift(codes_s[a:a + CH], sh, out=pref[a:a + CH])
        else:
            pref = codes_s
        mask = np.empty(n_ - 1, bool)
        np.not_equal(pref[1:], pref[:-1], out=mask)
        n_uniq = 1 + int(mask.sum())
        u = np.empty(n_uniq, np.uint64)
        cnt = np.empty(n_uniq, np.int32)
        u[0] = pref[0]
        k, prev_s = 1, 0  # prev_s = element index where the open segment starts
        for a in range(0, n_ - 1, CH):
            idx = np.flatnonzero(mask[a:a + CH])
            if idx.size:
                p = idx.astype(np.int64) + a       # global change positions
                starts = p + 1                     # new-segment element indices
                u[k:k + p.size] = pref[starts]     # new segment values
                cnt[k - 1:k - 1 + p.size] = np.diff(
                    np.concatenate((np.array([prev_s], np.int64), starts)))
                prev_s = int(starts[-1])
                k += p.size
                del p, starts
            del idx
        cnt[k - 1] = n_ - prev_s
        del mask
        if d < MAX_DEPTH:
            del pref
        return u, cnt

    leaf_codes, leaf_depth, leaf_counts = [], [], []
    u_d, cnt_d = level(MAX_DEPTH)
    u_p = cnt_p = None
    for d in range(MAX_DEPTH, -1, -1):
        if d > 0:
            u_p, cnt_p = level(d - 1)
            parent = u_d >> np.uint64(3)
            pi = np.searchsorted(u_p, parent)
            pcount = cnt_p[np.clip(pi, 0, len(cnt_p) - 1)]
            eligible = (cnt_d <= args.leaf_cap) | (d == MAX_DEPTH)
            sel = eligible & (pcount > args.leaf_cap)
            del parent, pi, pcount
        else:
            sel = cnt_d <= args.leaf_cap
        leaf_codes.append(u_d[sel])
        leaf_depth.append(np.full(int(sel.sum()), d, np.uint8))
        leaf_counts.append(cnt_d[sel])
        if d > 0:
            u_d, cnt_d = u_p, cnt_p
        del sel
    del u_d, cnt_d, u_p, cnt_p

    leaf_code = np.concatenate(leaf_codes)
    leaf_d = np.concatenate(leaf_depth)
    leaf_cnt = np.concatenate(leaf_counts)
    n_leaves = len(leaf_code)
    log_mem("after leaf cutting")

    # assign in block (depth-descending) id space, then remap through the
    # DEPTH-ALIGNED code sort (mixed-depth raw numeric order != spatial order)
    offs, acc = {}, 0
    for d in range(MAX_DEPTH, -1, -1):
        offs[d] = acc
        acc += int(np.sum(leaf_d == d))
    assign_old = np.full(n, -1, np.int32)  # s006: leaf ids < 2^31 — 44 MB saved
    for d in range(0, MAX_DEPTH + 1):
        m = leaf_d == d
        if not m.any():
            continue
        sub = leaf_code[m]
        pref = codes_s >> np.uint64(3 * (MAX_DEPTH - d))
        idx = np.searchsorted(sub, pref)
        hit = idx < len(sub)
        lc = sub[np.clip(idx, 0, len(sub) - 1)]
        ok = hit & (lc == pref)
        assign_old[ok] = offs[d] + idx[ok]
        del sub, pref, idx, hit, lc, ok, m
    aligned = leaf_code.astype(np.uint64) << np.uint64(
        3 * (MAX_DEPTH - leaf_d.astype(np.uint64)))
    sort_i = np.argsort(aligned, kind="stable")
    rank = np.empty(n_leaves, np.int32)
    rank[sort_i] = np.arange(n_leaves)
    leaf_code, leaf_d, leaf_cnt = leaf_code[sort_i], leaf_d[sort_i], leaf_cnt[sort_i]
    assign = rank[assign_old]
    del assign_old, aligned, rank, sort_i
    log_mem("after assign")

    bad = (assign < 0) | (assign >= n_leaves)
    chk = np.bincount(assign[~bad], minlength=n_leaves) if (~bad).any() \
        else np.zeros(n_leaves, np.int64)
    mis = np.nonzero(chk != leaf_cnt)[0]
    print(f"leaves: {n_leaves} (cap {args.leaf_cap}); max leaf {leaf_cnt.max()}; "
          f"UNASSIGNED={int(bad.sum())} MISMATCHED={len(mis)}", flush=True)
    assert not bad.sum() and not len(mis), "leaf assignment inconsistent"

    # per-leaf brightest (min G): chunked over the sorted order
    starts = np.searchsorted(assign, np.arange(n_leaves))
    ends = np.append(starts[1:], n)
    mags_s = stars["mag"][order]  # f32 gather, ~34 MB
    brightest_leaf = np.empty(n_leaves, np.float32)
    for i in range(n_leaves):
        s_, e_ = starts[i], ends[i]
        brightest_leaf[i] = mags_s[s_:e_].min() if e_ > s_ else 99.0
    del mags_s

    # ── node table, keys (code, depth): raw code is ambiguous (root == oct 0) ─
    nodes = {}
    for i, (lc, d, cnt, br) in enumerate(zip(leaf_code, leaf_d, leaf_cnt, brightest_leaf)):
        nodes[(int(lc), int(d))] = dict(depth=int(d), count=int(cnt), leaf=i,
                                        brightest=float(br), is_leaf=True)
        for ad in range(int(d) - 1, -1, -1):
            ac = int(lc) >> (3 * (int(d) - ad))
            nodes.setdefault((ac, ad), dict(depth=ad, count=0, leaf=0,
                                            brightest=99.0, is_leaf=False))
    for key in sorted(nodes, key=lambda k: (-k[1], k[0])):  # deep -> shallow
        code, d = key
        nd = nodes[key]
        if nd["is_leaf"]:
            continue
        tot, br = 0, 99.0
        for k in range(8):
            ch = nodes.get(((code << 3) | k, d + 1))
            if ch:
                tot += ch["count"]
                br = min(br, ch["brightest"])
        nd["count"], nd["brightest"] = tot, br

    codes_list = sorted(nodes, key=lambda k: (k[1], k[0]))  # root (0,0) first
    index = {key: i for i, key in enumerate(codes_list)}
    tbl = np.zeros(len(codes_list), NODE)
    for i, (code, d) in enumerate(codes_list):
        nd = nodes[(code, d)]
        c, rad = cell_geom(code, d, center, half)
        kids = [-1] * 8
        if not nd["is_leaf"]:
            for k in range(8):
                kk = ((code << 3) | k, d + 1)
                if kk in index:
                    kids[k] = index[kk]
        tbl[i] = (code, d, 1 if nd["is_leaf"] else 0, nd["count"],
                  nd["leaf"] if nd["is_leaf"] else 0, nd["brightest"],
                  c[0], c[1], c[2], rad, 0, kids)
    del nodes, index

    # ── write artifacts (chunked gather; sorted stars never materialised) ───
    stars_path = os.path.join(OUT, "stars.bin")
    h = hashlib.sha256()
    with open(stars_path, "wb") as fh:
        for a in range(0, n, 1 << 20):
            chunk = stars[order[a:a + (1 << 20)]]
            chunk.tofile(fh)
            h.update(chunk.tobytes())
            del chunk
    tbl.tofile(os.path.join(OUT, "nodes.bin"))
    idx = np.zeros(n_leaves, np.dtype([("start", "<u8"), ("count", "<u4"), ("pad", "<u4")]))
    idx["start"] = starts
    idx["count"] = ends - starts
    idx.tofile(os.path.join(OUT, "leaves.idx"))
    meta = {
        "schema": "galaxia-t0-octree-v0",
        "partitions": nparts, "stars": int(n), "leaves": int(n_leaves),
        "nodes": int(len(codes_list)), "leaf_cap": args.leaf_cap, "max_depth": MAX_DEPTH,
        "root_center_pc": [float(v) for v in center], "root_half_pc": half,
        "record_bytes": 32, "node_bytes": int(NODE.itemsize),
        "sha256_stars": h.hexdigest(),
    }
    json.dump(meta, open(os.path.join(OUT, "tree_meta.json"), "w"), indent=1)
    try:
        os.remove(os.path.join(OUT, ".stars.tmp"))
    except OSError:
        pass
    print(json.dumps({k: meta[k] for k in ("stars", "leaves", "nodes", "partitions")}),
          flush=True)
    print("OCTREE BUILD DONE", flush=True)


if __name__ == "__main__":
    main()
