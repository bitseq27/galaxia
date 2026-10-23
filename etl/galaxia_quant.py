"""galaxia_quant — 32 B/star quantised record v0 (see fetch_orion.py docstring).

Layout (little-endian, 32 bytes):
    x, y, z : f32  pc, Sun-centred ICRS equatorial (It.0 shortcut frame)
    mag     : f32  Gaia G
    r, g, b : f32  crude linear colour from bp_rp (Harre-Heller chain = It.2)
    flags   : u32  bit0 provenance (0 = gaia_dr3_real), bit1 = has parallax

The authoritative §8.4 GPU schema (Morton keys, LOD metadata, provenance enum)
lands with the It.1 octree builder; this exists so the DAY-0 fetch can bake and
delete raw partitions immediately (§8.4.1 rule 1).
"""
import math
import struct

AU_IN_PC = 206264.8062470964  # unused here (positions stored in pc) — kept for parity


def bp_rp_to_rgb_crude(bp_rp):
    if bp_rp != bp_rp:  # NaN
        return (1.0, 0.25, 0.9)  # UNKNOWN marker — never invent data
    anchors = [
        (-0.5, (0.62, 0.72, 1.00)),
        (0.0, (0.76, 0.83, 1.00)),
        (0.5, (0.92, 0.93, 1.00)),
        (0.82, (1.00, 1.00, 0.96)),
        (1.15, (1.00, 0.91, 0.78)),
        (1.8, (1.00, 0.74, 0.55)),
        (3.5, (1.00, 0.55, 0.42)),
    ]
    x = min(max(bp_rp, anchors[0][0]), anchors[-1][0])
    for (x0, c0), (x1, c1) in zip(anchors, anchors[1:]):
        if x <= x1:
            t = (x - x0) / (x1 - x0)
            return tuple(c0[i] + (c1[i] - c0[i]) * t for i in range(3))
    return anchors[-1][1]


def record(ra_deg, dec_deg, dist_pc, mag, bp_rp, has_parallax):
    ra = math.radians(ra_deg)
    dec = math.radians(dec_deg)
    cd = math.cos(dec)
    x = dist_pc * cd * math.cos(ra)
    y = dist_pc * cd * math.sin(ra)
    z = dist_pc * math.sin(dec)
    r, g, b = bp_rp_to_rgb_crude(bp_rp)
    flags = 0 | (0 if has_parallax else 2)
    return struct.pack("<4f3fI", x, y, z, mag, r, g, b, flags)


RECORD_BYTES = 32
