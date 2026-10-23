//! Canonical units — PLAN §5.1. Newtypes are [MUST]: unit confusion across a
//! 20-order-of-magnitude scene is unrecoverable otherwise.

use serde::{Deserialize, Serialize};

/// 1 parsec in AU (IAU definition: 648000/π AU).
pub const AU_IN_PC: f64 = 206_264.806_247_096_400.0e-9; // ≈ 206264.806
/// Speed of light in AU per day.
pub const C_AU_PER_DAY: f64 = 173.144_632_674;
/// J2000.0 epoch as Julian Date.
pub const JD_J2000: f64 = 2_451_545.0;

macro_rules! unit_newtype {
    ($name:ident, $inner:ty, $doc:literal) => {
        #[doc = $doc]
        #[derive(Copy, Clone, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
        pub struct $name(pub $inner);

        impl $name {
            #[inline]
            pub fn get(self) -> $inner {
                self.0
            }
        }

        impl std::ops::Add for $name {
            type Output = Self;
            #[inline]
            fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }
        }
        impl std::ops::Sub for $name {
            type Output = Self;
            #[inline]
            fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }
        }
        impl std::ops::Neg for $name {
            type Output = Self;
            #[inline]
            fn neg(self) -> Self {
                Self(-self.0)
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{} {}", self.0, stringify!($name))
            }
        }
    };
}

unit_newtype!(Au, f64, "Absolute/relative position unit (§5.1). ~1 mm precision at 100 kpc in f64.");
unit_newtype!(Pc, f64, "Parsec — the public-API distance unit (§5.1). Convert to ly only at display.");
unit_newtype!(Km, f64, "Kilometre — planetary-scale work (§9).");
unit_newtype!(JulianDay, f64, "f64 days since J2000.0 TDB = JD − 2451545.0 (§5.1).");
unit_newtype!(Mag, f32, "Magnitude. Gaia G is its own system — never mix G and V without a transform.");
unit_newtype!(Kelvin, f32, "Temperature.");
unit_newtype!(Lsol, f32, "Luminosity in solar units (log-encoded in GPU buffers, §5.1).");
unit_newtype!(Msol, f32, "Stellar mass in solar units.");

impl Au {
    #[inline]
    pub fn from_pc(pc: Pc) -> Self {
        Self(pc.0 * AU_IN_PC)
    }
    #[inline]
    pub fn to_pc(self) -> Pc {
        Pc(self.0 / AU_IN_PC)
    }
    #[inline]
    pub fn from_km(km: Km) -> Self {
        Self(km.0 / 149_597_870.7)
    }
}

impl Pc {
    #[inline]
    pub fn to_au(self) -> Au {
        Au::from_pc(self)
    }
    #[inline]
    pub fn from_arcsec_parallax(parallax_arcsec: f64) -> Option<Self> {
        // PLAN: NEVER use 1/parallax for distances in science output — Bailer-Jones
        // geometric distances are the source of truth. This helper exists only for
        // quick-look sanity checks and tests.
        if parallax_arcsec > 1e-12 {
            Some(Self(1.0 / parallax_arcsec))
        } else {
            None
        }
    }
}

impl JulianDay {
    #[inline]
    pub fn from_jd(jd: f64) -> Self {
        Self(jd - JD_J2000)
    }
    #[inline]
    pub fn to_jd(self) -> f64 {
        self.0 + JD_J2000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pc_au_roundtrip() {
        let d = Au::from_pc(Pc(137.0));
        let back = d.to_pc();
        assert!((back.0 - 137.0).abs() < 1e-9, "{back:?}");
    }

    #[test]
    fn au_in_pc_is_correct() {
        // 648000/pi
        assert!((AU_IN_PC - 206_264.806_247_096_4).abs() < 1e-6, "{AU_IN_PC}");
    }

    #[test]
    fn c_light_au_per_day() {
        // 299792.458 km/s * 86400 s / 149597870.7 km
        let expected = 299_792.458 * 86_400.0 / 149_597_870.7;
        assert!((C_AU_PER_DAY - expected).abs() / expected < 1e-9);
    }
}
