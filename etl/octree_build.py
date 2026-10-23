#!/usr/bin/env python3
"""etl/octree_build.py — T0 octree bake (PLAN §H2, §16.4 It.1).

Reads the quantised partitions (etl/build/n32_*.bin, 32 B/star v0), Morton-sorts
them, cuts leaves at ~LEAF_CAP stars, and emits the streaming artifact set:

    etl/build/t0/tree_meta.json   root cube, schema, counts, sha256
    etl/build/t0/nodes.bin        node metadata (resident set, PLAN §H2)
    etl/build/t0/stars.bin        all stars in Morton order (32 B each)
    etl/build/t0/leaves.idx       leaf -> (start record, count)

Node record v0 (80 B):
    morton   u64      cell code (depth bits, root=0)
    depth    u32
    flags    u32      bit0 = is_leaf
    count    u32
    leaf_id  u32      index into leaves.idx (leaves only)
    brightest f32     min Gaia G in subtree (LOD criterion input, §H2)
    center   f32x3    cell centre, pc, Sun-centred ICRS
    radius   f32      cell half-diagonal, pc
    children i32x8    node indices, -1 = empty

Deterministic: stable sorts, fixed tie-breaks, sha256 over outputs.
"""
import argparse, hashlib, json, os, struct, sys
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
                 ("children", "<i4", (8,))])


def load_partitions(limit_parts=0):
    files = sorted(f for f in os.listdir(SRC) if f.startswith("n32_") and f.endswith(".bin"))
    if limit_parts:
        files = files[:limit_parts]
    blobs = []
    n = 0
    for f in files:
        p = os.path.join(SRC, f)
        sz = os.path.getsize(p)
        if sz == 0:
            continue
        blobs.append(np.fromfile(p, dtype=REC))
        n += sz // 32
    if not blobs:
        sys.exit("no partitions baked yet")
    return np.concatenate(blobs), len(files)


def morton3(x, y, z, depth):
    """Interleave-bit Morton codes for integer coords in [0, 2^depth)."""
    code = np.zeros(len(x), dtype=np.uint64)
    for d in range(depth):
        bit = np.uint64(1) << np.uint64(d)
        bx = (x & bit.astype(x.dtype)) != 0
        by = (y & bit.astype(y.dtype)) != 0
        bz = (z & bit.astype(z.dtype)) != 0
        code |= (bx.astype(np.uint64) << np.uint64(3 * d)) \
            | (by.astype(np.uint64) << np.uint64(3 * d + 1)) \
            | (bz.astype(np.uint64) << np.uint64(3 * d + 2))
    return code


def group_counts(codes):
    u, inv, cnt = np.unique(codes, return_inverse=True, return_counts=True)
    return u, inv, cnt


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--limit-parts", type=int, default=0)
    ap.add_argument("--leaf-cap", type=int, default=LEAF_CAP)
    args = ap.parse_args()
    os.makedirs(OUT, exist_ok=True)

    stars, nparts = load_partitions(args.limit_parts)
    n = len(stars)
    print(f"loaded {n} stars from {nparts} partitions", flush=True)

    # ── root cube ────────────────────────────────────────────────────────────
    xyz0 = np.column_stack([stars["x"], stars["y"], stars["z"]]).astype(np.float64)
    lo = xyz0.min(axis=0)
    hi = xyz0.max(axis=0)
    center = (lo + hi) / 2.0
    half = float(np.max(hi - center)) * 1.0001

    # integer coords at MAX_DEPTH
    xyz = xyz0
    q = np.clip(((xyz - center) / (2 * half) + 0.5) * (1 << MAX_DEPTH), 0,
                (1 << MAX_DEPTH) - 1).astype(np.uint32)
    codes = morton3(q[:, 0], q[:, 1], q[:, 2], MAX_DEPTH)

    # sort stars by (code, original index) — deterministic Morton order
    order = np.lexsort((np.arange(n), codes))
    codes_s = codes[order]
    stars_s = stars[order]

    # ── per-depth group counts (bottom-up leaf cutting) ─────────────────────
    counts_at = {}
    for d in range(MAX_DEPTH, -1, -1):
        shift = np.uint64(3 * (MAX_DEPTH - d))
        pref = codes_s >> shift if d < MAX_DEPTH else codes_s
        u, inv, cnt = group_counts(pref)
        counts_at[d] = (u, cnt)

    # leaves: deepest nodes with count <= cap whose parent exceeds cap (or root)
    leaf_codes = []
    leaf_depth = []
    leaf_counts = []
    for d in range(MAX_DEPTH, -1, -1):
        u, cnt = counts_at[d]
        eligible = (cnt <= args.leaf_cap) | (d == MAX_DEPTH)
        if d == 0:
            sel = eligible
        else:
            pu, pcnt = counts_at[d - 1]
            parent = u >> np.uint64(3)
            idx = np.searchsorted(pu, parent)
            pcount = pcnt[np.clip(idx, 0, len(pcnt) - 1)]
            sel = eligible & (pcount > args.leaf_cap)
        leaf_codes.append(u[sel])
        leaf_depth.append(np.full(int(sel.sum()), d, np.uint8))
        leaf_counts.append(cnt[sel])
    # a star's leaf = the selected node that prefixes its code; resolve by
    # walking depths shallow->deep with searchsorted (vectorised per depth)
    leaf_code = np.concatenate(leaf_codes)
    leaf_d = np.concatenate(leaf_depth)
    leaf_cnt = np.concatenate(leaf_counts)
    n_leaves = len(leaf_code)

    # Block (concat) order is depth-descending; record per-depth base offsets,
    # assign stars in THAT id space, then remap ids through the code-sort
    # permutation so ids match the emitted (code-sorted) leaf table.
    offs, acc = {}, 0
    for blk in leaf_codes:  # appended MAX_DEPTH..0
        d_blk = None
        acc += 0
    acc = 0
    for d in range(MAX_DEPTH, -1, -1):
        offs[d] = acc
        acc += int(np.sum(leaf_d == d))
    assign_old = np.full(n, -1, np.int64)
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

    # Mixed-depth codes must be compared DEPTH-ALIGNED: numeric order of raw
    # codes does not match spatial interval order across depths.
    aligned = leaf_code.astype(np.uint64) << np.uint64(3 * (MAX_DEPTH - leaf_d.astype(np.uint64)))
    sort_i = np.argsort(aligned, kind="stable")
    rank = np.empty(n_leaves, np.int64)
    rank[sort_i] = np.arange(n_leaves)
    leaf_code, leaf_d, leaf_cnt = leaf_code[sort_i], leaf_d[sort_i], leaf_cnt[sort_i]
    assign = rank[assign_old]

    bad = (assign < 0) | (assign >= n_leaves)
    chk = np.bincount(assign[~bad], minlength=n_leaves) if (~bad).any() else np.zeros(n_leaves, np.int64)
    mis = np.nonzero(chk != leaf_cnt)[0]
    print(f"leaves: {n_leaves} (cap {args.leaf_cap}); max leaf {leaf_cnt.max()}; "
          f"UNASSIGNED={int(bad.sum())} MISMATCHED={len(mis)}", flush=True)
    assert not bad.sum() and not len(mis), "leaf assignment inconsistent"

    # per-leaf brightest (min G) via reduceat on contiguous ranges
    starts = np.searchsorted(assign, np.arange(n_leaves))
    ends = np.append(starts[1:], n)
    brightest_leaf = np.array([
        stars_s["mag"][s:e].min() if e > s else 99.0 for s, e in zip(starts, ends)
    ], np.float32)

    # ── node table: leaves + all ancestors, children indices ────────────────
    def cell_geom(code, d):
        """Cell centre (pc) and half-diagonal for a Morton cell at depth d."""
        c = np.array(center, np.float64)
        s_ = 2 * half
        for bit in range(d):
            s_ /= 2
            sh = 3 * (d - 1 - bit)
            off = np.array([(code >> sh) & 1, (code >> (sh + 1)) & 1,
                            (code >> (sh + 2)) & 1], np.float64) - 0.5
            c = c + off * s_
        return c, float(np.sqrt(3) * s_ / 2)

    nodes = {}   # (code, depth) -> dict   (code alone is ambiguous: root == octant 0)
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
        c, rad = cell_geom(code, d)
        kids = [-1] * 8
        if not nd["is_leaf"]:
            for k in range(8):
                kk = ((code << 3) | k, d + 1)
                if kk in index:
                    kids[k] = index[kk]
        tbl[i] = (code, d, 1 if nd["is_leaf"] else 0, nd["count"],
                  nd["leaf"] if nd["is_leaf"] else 0, nd["brightest"],
                  c[0], c[1], c[2], rad, kids)

    # ── write artifacts ─────────────────────────────────────────────────────
    stars_path = os.path.join(OUT, "stars.bin")
    stars_s.tofile(stars_path)
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
        "sha256_stars": hashlib.sha256(open(stars_path, "rb").read()).hexdigest(),
    }
    json.dump(meta, open(os.path.join(OUT, "tree_meta.json"), "w"), indent=1)
    print(json.dumps({k: meta[k] for k in ("stars", "leaves", "nodes", "partitions")}), flush=True)
    print("OCTREE BUILD DONE", flush=True)


if __name__ == "__main__":
    main()
