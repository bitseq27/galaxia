//! Coordinate maths for the Iteration-0 skeleton.
//!
//! ICRS equatorial (ra, dec degrees; distance pc) → Cartesian AU, J2000.0
//! equinox, per PLAN §5.1/§5.2. The skeleton's single Sun-centred f64 frame
//! is the documented It.0 shortcut (retired in It.3).

use glam::DVec3;

/// Equatorial ICRS (degrees, degrees, parsec) → Cartesian AU (J2000 equatorial frame).
/// X → vernal equinox, Z → north celestial pole.
#[inline]
pub fn equatorial_to_cartesian_au(ra_deg: f64, dec_deg: f64, dist_pc: f64) -> DVec3 {
    let ra = ra_deg.to_radians();
    let dec = dec_deg.to_radians();
    let d_au = dist_pc * crate::units::AU_IN_PC;
    let cos_dec = dec.cos();
    DVec3::new(d_au * cos_dec * ra.cos(), d_au * cos_dec * ra.sin(), d_au * dec.sin())
}

/// Distance in parsec from parallax in milliarcseconds, with a guard.
/// NOTE (PLAN §13): science output must use Bailer-Jones distances, never
/// 1/parallax. For the bright (G<10) skeleton subset parallaxes are high-S/N,
/// so 1/ϖ is acceptable *for Iteration 0 only*; the ETL will replace it.
#[inline]
pub fn parallax_mas_to_pc(parallax_mas: f64) -> Option<f64> {
    if parallax_mas.is_finite() && parallax_mas > 0.05 {
        Some(1000.0 / parallax_mas)
    } else {
        None
    }
}

/// Angular separation (radians) between two sky positions (degrees).
pub fn angular_separation_deg(ra1: f64, dec1: f64, ra2: f64, dec2: f64) -> f64 {
    let (r1, d1) = (ra1.to_radians(), dec1.to_radians());
    let (r2, d2) = (ra2.to_radians(), dec2.to_radians());
    let cos_sep = d1.sin() * d2.sin() + d1.cos() * d2.cos() * (r1 - r2).cos();
    cos_sep.clamp(-1.0, 1.0).acos().to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_vectors() {
        let x = equatorial_to_cartesian_au(0.0, 0.0, 1.0);
        let z = equatorial_to_cartesian_au(0.0, 90.0, 1.0);
        assert!((x.x - crate::units::AU_IN_PC).abs() < 1e-6);
        assert!(x.y.abs() < 1e-6 && x.z.abs() < 1e-6);
        assert!((z.z - crate::units::AU_IN_PC).abs() < 1e-3);
    }

    #[test]
    fn separation_alnilam_to_betelgeuse() {
        // Alnilam (84.053, -1.202), Betelgeuse (88.793, 7.407) → ~9.6° apart.
        let sep = angular_separation_deg(84.053, -1.202, 88.793, 7.407);
        assert!((sep - 9.6).abs() < 0.2, "sep={sep}");
    }

    #[test]
    fn parallax_guard() {
        assert!(parallax_mas_to_pc(0.0).is_none());
        assert!(parallax_mas_to_pc(-2.0).is_none());
        assert!((parallax_mas_to_pc(100.0).unwrap() - 10.0).abs() < 1e-9);
    }
}
