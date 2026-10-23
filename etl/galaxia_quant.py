"""galaxia_quant — 32 B/star quantised record v0 (see fetch_orion.py docstring).

Layout (little-endian, 32 bytes):
    x, y, z : f32  pc, Sun-centred ICRS equatorial (It.0 shortcut frame)
    mag     : f32  Gaia G
    r, g, b : f32  crude linear colour from bp_rp (Harre-Heller chain = It.2)
    flags   : u32  bit0 provenance (0 = gaia_dr3_real, 1 = synthetic)
                   bit1 NO usable Gaia DR3 parallax (distance not from DR3 plx;
                        s005 doc fix: the field was originally documented as
                        "has parallax" but always implemented as its inverse —
                        first exercised by the s005 literature rows)
                   bit2 distance_from_literature (s005: SIMBAD/HIP compiled
                        distance; see etl/norm/literature_distances.csv origin)

The authoritative §8.4 GPU schema (Morton keys, LOD metadata, provenance enum)
lands with the It.1 octree builder; this exists so the DAY-0 fetch can bake and
delete raw partitions immediately (§8.4.1 rule 1).
"""
import bisect
import math
import struct

AU_IN_PC = 206264.8062470964  # unused here (positions stored in pc) — kept for parity

FLAG_NO_GAIA_PLX = 2
FLAG_DIST_FROM_LITERATURE = 4


UNKNOWN_RGB = (1.0, 0.25, 0.9)  # NaN/unknown marker — never invent data
_HH_KEYS = None


def bp_rp_to_linear_srgb(bp_rp):
    """Gaia (BP-RP) -> linear sRGB via the GENERATED Harre-Heller (2021) LUT
    (etl/make_hh_lut.py; PM2013 dwarf bridge; HH Table 5 log-g step; TLUSTY
    tail transcribed). Mirror of galaxia-core color::bp_rp_to_linear_srgb —
    both emitted by the same generator, semantics identical: NaN -> UNKNOWN,
    clamp outside table, piecewise-linear between anchors.
    Replaced bp_rp_to_rgb_crude in session 007 (It.2, PLAN §7.1 step 4)."""
    global _HH_KEYS
    if bp_rp != bp_rp:  # NaN
        return UNKNOWN_RGB
    if _HH_KEYS is None:
        _HH_KEYS = [p[0] for p in HH_LUT]
    if bp_rp <= _HH_KEYS[0]:
        return HH_LUT[0][1]
    if bp_rp >= _HH_KEYS[-1]:
        return HH_LUT[-1][1]
    hi = bisect.bisect_right(_HH_KEYS, bp_rp)
    (x0, c0) = HH_LUT[hi - 1]
    (x1, c1) = HH_LUT[hi]
    t = min(max((bp_rp - x0) / (x1 - x0), 0.0), 1.0)
    return tuple(c0[i] + (c1[i] - c0[i]) * t for i in range(3))


def record(ra_deg, dec_deg, dist_pc, mag, bp_rp, has_parallax, extra_flags=0):
    ra = math.radians(ra_deg)
    dec = math.radians(dec_deg)
    cd = math.cos(dec)
    x = dist_pc * cd * math.cos(ra)
    y = dist_pc * cd * math.sin(ra)
    z = dist_pc * math.sin(dec)
    r, g, b = bp_rp_to_linear_srgb(bp_rp)
    flags = extra_flags | (0 if has_parallax else FLAG_NO_GAIA_PLX)
    return struct.pack("<4f3fI", x, y, z, mag, r, g, b, flags)


RECORD_BYTES = 32

# --- GENERATED Harre-Heller LUT (etl/make_hh_lut.py) DO NOT EDIT ---
HH_LUT = [
    (-0.32, (0.361, 0.489, 1.0)),
    (-0.12, (0.4553, 0.5713, 1.0)),
    (-0.087, (0.4695, 0.584, 1.0)),
    (-0.037, (0.4833, 0.5953, 1.0)),
    (0.005, (0.503, 0.612, 1.0)),
    (0.068, (0.525, 0.629, 1.0)),
    (0.11, (0.546, 0.645, 1.0)),
    (0.166, (0.5675, 0.661, 1.0)),
    (0.222, (0.606, 0.69, 1.0)),
    (0.263, (0.636, 0.712, 1.0)),
    (0.32, (0.674, 0.738, 1.0)),
    (0.327, (0.692, 0.75, 1.0)),
    (0.377, (0.725, 0.773, 1.0)),
    (0.434, (0.763, 0.799, 1.0)),
    (0.49, (0.792, 0.819, 1.0)),
    (0.518, (0.823, 0.84, 1.0)),
    (0.546, (0.844, 0.855, 1.0)),
    (0.587, (0.869, 0.871, 1.0)),
    (0.64, (0.909, 0.8995, 1.0)),
    (0.694, (0.955, 0.931, 1.0)),
    (0.719, (1.0, 0.96, 0.998)),
    (0.767, (1.0, 0.951, 0.967)),
    (0.803, (1.0, 0.94, 0.931)),
    (0.823, (1.0, 0.931, 0.905)),
    (0.841, (1.0, 0.922, 0.878)),
    (0.869, (1.0, 0.91, 0.845)),
    (0.9, (1.0, 0.898, 0.813)),
    (0.95, (1.0, 0.886, 0.783)),
    (0.983, (1.0, 0.872, 0.753)),
    (1.01, (1.0, 0.857, 0.722)),
    (1.21, (1.0, 0.7915, 0.6105)),
    (1.34, (1.0, 0.739, 0.533)),
    (1.43, (1.0, 0.696, 0.47)),
    (1.7, (1.0, 0.65, 0.389)),
    (1.73, (1.0, 0.641, 0.369)),
    (1.84, (1.0, 0.636, 0.354)),
    (1.97, (1.0, 0.635, 0.346)),
    (2.09, (1.0, 0.637, 0.34)),
    (2.23, (1.0, 0.635, 0.327)),
    (2.39, (1.0, 0.637, 0.322)),
    (2.5, (1.0, 0.638, 0.315)),
    (2.78, (1.0, 0.638, 0.308)),
    (2.94, (1.0, 0.638, 0.3)),
    (3.16, (1.0, 0.638, 0.293)),
    (3.71, (1.0, 0.641, 0.289)),
    (4.16, (1.0, 0.6465, 0.285)),
    (4.5, (1.0, 0.648, 0.286)),
    (4.72, (1.0, 0.607, 0.255)),
    (4.78, (1.0, 0.518, 0.179)),
    (4.86, (1.0, 0.491, 0.144)),
]
# --- END GENERATED Harre-Heller LUT ---
