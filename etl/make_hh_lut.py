#!/usr/bin/env python3
"""etl/make_hh_lut.py — session 007: generate the Harre-Heller (2021) colour LUT.

Chain (PLAN §7.1 step 4, prototype form):
    Gaia (BP-RP) --Harre&Heller(2021) Table 5 colours, joined on the spectral
    type label-- linear sRGB, with the BP-RP value of each type taken from the
    Pecaut & Mamajek (2013) empirical dwarf sequence.

Joining on SpT (not on Teff) is deliberate and measured: PM and HH disagree by
~50-100 K per type, and near the white-point crossing (F8-F9, 6100 K)
d(colour)/d(Teff) is steep enough that a 50 K offset costs dE2000 ~5 (measured
s007: F9V bridge-by-Teff = 5.23 vs the project gate of 3). Both published
tables agree on the SpT label, so the label is the faithful join key.

Inputs (committed under etl/norm/vendor/):
  hh2021_results_Z-0.txt  Zenodo 10.5281/zenodo.4090873 (CC-BY-4.0): full
                          PHOENIX [Fe/H]=0 grid (Teff, log g) -> linear RGB
  hh2021_table5.csv       HH2021 Table 5 (107 rows, SpT/Teff/log g/linear RGB)
                          transcribed from arXiv:2101.06254v2; EVERY row with
                          Teff <= 12000 is cross-validated here against the
                          Zenodo grid (exact 3-decimal match), which makes
                          transcription errors impossible to sneak through
  pm2013_eem_dwarf.txt    Pecaut & Mamajek empirical dwarf table (SpT, Bp-Rp)

Outputs (single source of truth — Rust and Python can never drift):
  crates/galaxia-core/src/color_hh_table.rs   pub const HH_LUT
  etl/galaxia_quant.py                        GENERATED block replaced in place
  etl/reports/hh_lut_s007.csv                 human-readable audit copy
"""
import csv
import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
HH_GRID = os.path.join(HERE, "norm", "vendor", "hh2021_results_Z-0.txt")
HH_T5 = os.path.join(HERE, "norm", "vendor", "hh2021_table5.csv")
PM = os.path.join(HERE, "norm", "vendor", "pm2013_eem_dwarf.txt")
RUST_OUT = os.path.normpath(
    os.path.join(HERE, "..", "crates", "galaxia-core", "src", "color_hh_table.rs"))
PY_TARGET = os.path.join(HERE, "galaxia_quant.py")
CSV_OUT = os.path.join(HERE, "reports", "hh_lut_s007.csv")

# Published-source conflicts, resolved in favour of the Zenodo dataset (the
# paper's "underlying data") over the typeset Table 5, with the evidence:
OVERRIDES = {
    ("F9V", 6100): "typeset Table 5 prints (0.96,0.998,1.0) but the Zenodo "
                   "grid row (same hex #fff4fe) is (1.0,0.96,0.998); the "
                   "typeset g=0.998 is a non-physical spike between the "
                   "6000 K (g=0.951) and 6200 K (g=0.931) rows — channel "
                   "rotation typo in typesetting (measured s007: the typo "
                   "costs dE2000 5.2 against the smooth chain)",
}


def srgb_to_lab(rgb):
    """Linear sRGB -> CIE Lab (D65)."""
    m = ((0.4124564, 0.3575761, 0.1804375),
         (0.2126729, 0.7151522, 0.0721750),
         (0.0193339, 0.1191920, 0.9503041))
    X, Y, Z = (sum(m[i][j] * rgb[j] for j in range(3)) for i in range(3))
    Xn, Yn, Zn = 0.95047, 1.0, 1.08883

    def f(t):
        return t ** (1 / 3) if t > 216 / 24389 else (24389 / 27 * t + 16) / 116
    fx, fy, fz = f(X / Xn), f(Y / Yn), f(Z / Zn)
    return (116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz))


def de2000(l1, l2):
    L1, a1, b1 = l1
    L2, a2, b2 = l2
    C1, C2 = math.hypot(a1, b1), math.hypot(a2, b2)
    Cb = (C1 + C2) / 2
    G = 0.5 * (1 - math.sqrt(Cb ** 7 / (Cb ** 7 + 25 ** 7)))
    a1p, a2p = (1 + G) * a1, (1 + G) * a2
    C1p, C2p = math.hypot(a1p, b1), math.hypot(a2p, b2)
    h1p = math.degrees(math.atan2(b1, a1p)) % 360
    h2p = math.degrees(math.atan2(b2, a2p)) % 360
    dLp, dCp = L2 - L1, C2p - C1p
    dhp = h2p - h1p
    if abs(dhp) > 180:
        dhp -= 360 * math.copysign(1, dhp)
    dHp = 2 * math.sqrt(C1p * C2p) * math.sin(math.radians(dhp) / 2)
    Lbp, Cbp = (L1 + L2) / 2, (C1p + C2p) / 2
    hbp = (h1p + h2p) / 2
    if abs(h1p - h2p) > 180:
        hbp += 180
    T = (1 - 0.17 * math.cos(math.radians(hbp - 30))
         + 0.24 * math.cos(math.radians(2 * hbp))
         + 0.32 * math.cos(math.radians(3 * hbp + 6))
         - 0.20 * math.cos(math.radians(4 * hbp - 63)))
    dth = 30 * math.exp(-((hbp - 275) / 25) ** 2)
    Cbp7 = Cbp ** 7
    Rc = 2 * math.sqrt(Cbp7 / (Cbp7 + 25 ** 7))
    Sl = 1 + 0.015 * (Lbp - 50) ** 2 / math.sqrt(20 + (Lbp - 50) ** 2)
    Sb, Sh = 1 + 0.045 * Cbp, 1 + 0.015 * Cbp * T
    Rt = -math.sin(math.radians(2 * dth)) * Rc
    return math.sqrt((dLp / Sl) ** 2 + (dCp / Sb) ** 2 + (dHp / Sh) ** 2
                     + Rt * (dCp / Sb) * (dHp / Sh))


def load_hh_grid():
    grid = {}  # (teff, logg) -> (r, g, b)
    row = re.compile(r"^(\d+)\s*&\s*([\d.]+)\s*&\s*0\.0\s*&\s*"
                     r"([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*&")
    with open(HH_GRID, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            m = row.match(line.strip())
            if m:
                t = int(m.group(1))
                lg = round(float(m.group(2)) * 2) / 2
                grid[(t, lg)] = (float(m.group(3)), float(m.group(4)),
                                 float(m.group(5)))
    return grid


def load_table5(grid):
    """Table 5 rows, cross-validated against the Zenodo PHOENIX grid."""
    groups = {}  # spt -> [rgb, ...] in table order
    order = []
    n_val = 0
    with open(HH_T5, newline="") as fh:
        for rec in csv.DictReader(fh):
            spt = rec["spt"].strip()
            teff = int(rec["teff"])
            logg = float(rec["logg"])
            rgb = (float(rec["r"]), float(rec["g"]), float(rec["b"]))
            if teff <= 12000:
                g = grid.get((teff, logg))
                if (spt, teff) in OVERRIDES:
                    rgb = g  # dataset wins over typeset; reason recorded above
                if g is None or any(abs(g[i] - rgb[i]) > 1e-9 for i in range(3)):
                    sys.exit(f"Table 5 transcription mismatch at {spt} "
                             f"{teff}/{logg}: csv={rgb} grid={g}")
                n_val += 1
            if spt not in groups:
                groups[spt] = []
                order.append(spt)
            groups[spt].append(rgb)
    cols = {s: tuple(round(sum(c[i] for c in groups[s]) / len(groups[s]), 4)
                     for i in range(3)) for s in order}
    return order, cols, n_val


def load_pm_bprp():
    out = {}
    spt = re.compile(r"^[OBAFGKM][0-9]*(\.[05])?V$")
    with open(PM, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            if line.startswith("#"):
                continue
            f = line.split()
            if len(f) < 13 or not spt.match(f[0]) or f[0] in out:
                continue
            try:
                out[f[0]] = float(f[11])
            except ValueError:
                continue  # Bp-Rp missing ("...") at the hot end
    return out


def main():
    grid = load_hh_grid()
    order, cols, n_val = load_table5(grid)
    pm = load_pm_bprp()
    if len(grid) < 800 or n_val < 60 or len(pm) < 60:
        sys.exit(f"vendor parse too small: grid={len(grid)} "
                 f"validated={n_val} pm={len(pm)}")

    lut = []  # (bp_rp, (r,g,b)) strictly increasing in bp_rp after the sort
    dropped = []
    for spt in order:  # Table 5 order is cool -> hot = bp_rp DESCENDING
        b = pm.get(spt)
        if b is None:
            dropped.append(spt)
            continue
        lut.append((round(b, 4), cols[spt]))
    lut.sort(key=lambda p: p[0])
    strict = []
    for p in lut:
        if strict and p[0] <= strict[-1][0]:
            dropped.append(f"dup-bprp {p[0]}")
            continue
        strict.append(p)
    lut = strict
    # Blue extension: HH colours converge for SpT earlier than ~B5 (paper
    # §3.1); give Gaia's bluest BP-RP (~-0.4, O stars have no PM Bp-Rp) a real
    # anchor instead of a flat clamp. Value = HH Table 5 O1V/55000 K endpoint.
    lut.insert(0, (round(lut[0][0] - 0.2, 4), cols["O1V"]))

    # Join checks against PUBLISHED Table 5 values, in the project's metric
    # (PLAN §17: dE2000 < 3).
    sun = min(lut, key=lambda p: abs(p[0] - pm["G2V"]))[1]
    de_sun = de2000(srgb_to_lab(sun), srgb_to_lab((1.0, 0.931, 0.905)))
    if de_sun >= 1.0:
        sys.exit(f"sun join check failed: dE2000={de_sun:.2f} (limit 1.0)")
    m95 = cols["M9.5V"]
    de_m95 = de2000(srgb_to_lab(lut[-1][1]), srgb_to_lab(m95))
    if de_m95 >= 3.0:
        sys.exit(f"red-end join check failed: dE2000={de_m95:.2f} (limit 3.0)")

    rows = ",\n    ".join(
        f"({b:.4}, [{r:.4}, {g:.4}, {bl:.4}])" for b, (r, g, bl) in lut)
    rust = f"""//! GENERATED by etl/make_hh_lut.py — DO NOT EDIT BY HAND.
//!
//! Harre & Heller (2021) "Digital color codes of stars" (arXiv:2101.06254,
//! CC-BY-4.0; Zenodo 10.5281/zenodo.4090873) linear-sRGB main-sequence
//! colours (Table 5, multi-row types averaged), keyed by Gaia BP-RP via the
//! Pecaut & Mamajek (2013) empirical dwarf sequence; join on the spectral
//! type label (see generator docstring for the measured reason).
//! First anchor is a blue-convergence extension (HH §3.1).
//! {len(lut)} anchors, BP-RP {lut[0][0]} .. {lut[-1][0]}; PLAN §7.1 step 4; BUILDLOG s007.

pub const HH_LUT: [(f32, [f32; 3]); {len(lut)}] = [
    {rows},
];
"""
    with open(RUST_OUT, "w") as fh:
        fh.write(rust)

    py_rows = ",\n    ".join(
        f"({b:.4}, ({r:.4}, {g:.4}, {bl:.4}))" for b, (r, g, bl) in lut)
    block = (f"# --- GENERATED Harre-Heller LUT (etl/make_hh_lut.py) "
             f"DO NOT EDIT ---\nHH_LUT = [\n    {py_rows},\n]\n"
             f"# --- END GENERATED Harre-Heller LUT ---\n")
    src = open(PY_TARGET).read()
    pat = re.compile(r"# --- GENERATED Harre-Heller LUT.*?# --- END GENERATED "
                     r"Harre-Heller LUT ---\n", re.S)
    if pat.search(src):
        src = pat.sub(lambda m: block, src, count=1)
    else:
        src += "\n" + block
    with open(PY_TARGET, "w") as fh:
        fh.write(src)

    with open(CSV_OUT, "w") as fh:
        fh.write("bp_rp,r,g,b\n")
        for b, (r, g, bl) in lut:
            fh.write(f"{b:.4},{r:.4},{g:.4},{bl:.4}\n")

    print(f"LUT anchors={len(lut)} bp_rp {lut[0][0]}..{lut[-1][0]}; "
          f"grid-validated rows={n_val}; dropped types={dropped}")
    print(f"join checks: sun dE2000={de_sun:.2f}, red end dE2000={de_m95:.2f}")
    print("WROTE rust table + python mirror + audit csv")


if __name__ == "__main__":
    main()
