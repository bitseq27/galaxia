//! Camera rig — It.0 skeleton version (PLAN §10.1 lands properly later).
//!
//! f64 Sun-centred position in parsecs (the documented It.0 single-frame
//! shortcut — retired in It.3). Exponential speed control (decades of AU/s),
//! damped rotation toward a target yaw/pitch.

use glam::{DVec3, Mat4};

pub struct Camera {
    /// Position, parsecs, Sun-centred ICRS-equatorial (It.0 shortcut frame).
    pub pos_pc: DVec3,
    /// Yaw/pitch in radians (current, damped).
    pub yaw: f64,
    pub pitch: f64,
    /// Yaw/pitch targets (input moves these; damping follows).
    pub yaw_target: f64,
    pub pitch_target: f64,
    /// log10(speed in AU per second). Scroll changes this by ±0.1/step.
    pub speed_exp: f32,
    /// Rotation damping rate (1/s) — critically-damped-ish exponential approach.
    pub rot_damping: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pos_pc: DVec3::ZERO,
            // Look toward the Orion cone centre: RA 83.82°, Dec −5.39°.
            yaw: -(83.82f64).to_radians(),
            pitch: (-5.39f64).to_radians(),
            yaw_target: -(83.82f64).to_radians(),
            pitch_target: (-5.39f64).to_radians(),
            speed_exp: 2.0, // 100 AU/s ≈ 0.0005 pc/s — gentle local drift
            rot_damping: 12.0,
        }
    }
}

impl Camera {
    /// Current speed in pc/s.
    #[inline]
    pub fn speed_pc_s(&self) -> f64 {
        10f64.powf(self.speed_exp as f64) / galaxia_core::units::AU_IN_PC
    }

    /// Forward unit vector (where the camera looks), in the equatorial frame.
    pub fn forward(&self) -> DVec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        DVec3::new(cy * cp, sy * cp, sp)
    }

    pub fn right(&self) -> DVec3 {
        let (sy, cy) = self.yaw.sin_cos();
        DVec3::new(-sy, cy, 0.0)
    }

    pub fn up(&self) -> DVec3 {
        self.forward().cross(self.right()).normalize()
    }

    /// Integrate one frame of movement + rotation damping.
    pub fn update(&mut self, dt: f64, move_dir: DVec3, look_delta: (f64, f64)) {
        // Damped rotation: targets move instantly from input; actual angles
        // approach exponentially ("damped rotation" per It.0 spec).
        self.yaw_target += look_delta.0;
        self.pitch_target = (self.pitch_target + look_delta.1).clamp(
            -std::f64::consts::FRAC_PI_2 * 0.999,
            std::f64::consts::FRAC_PI_2 * 0.999,
        );
        let k = 1.0 - (-self.rot_damping * dt).exp();
        self.yaw += (self.yaw_target - self.yaw) * k;
        self.pitch += (self.pitch_target - self.pitch) * k;

        // Exponential speed flight.
        let v = self.speed_pc_s() * dt;
        let fwd = self.forward();
        let right = self.right();
        let up = DVec3::Z; // world up for the skeleton; camera-relative in §10.1
        self.pos_pc += fwd * (move_dir.x * v) + right * (move_dir.y * v) + up * (move_dir.z * v);
    }

    /// Look at a sky position (RA/Dec degrees) — used by scene presets.
    pub fn look_at_equatorial(&mut self, ra_deg: f64, dec_deg: f64) {
        self.yaw = -ra_deg.to_radians();
        self.pitch = dec_deg.to_radians();
        self.yaw_target = self.yaw;
        self.pitch_target = self.pitch;
    }

    /// View-projection matrix, f64 camera-relative then narrowed to f32.
    /// Reversed-Z infinite-far (PLAN §5.4) via glam. `near_pc` is the derived
    /// near plane — the skeleton uses a fixed small value; It.3 derives it.
    pub fn view_proj_f32(&self, aspect: f32, fov_y_rad: f32, near_pc: f32) -> Mat4 {
        let view = Mat4::from_rotation_pitch(-(self.pitch as f32))
            * Mat4::from_rotation_yaw(-(self.yaw as f32))
            * Mat4::from_translation(-self.pos_pc.as_vec3());
        // NOTE: -cam.pos in f32 is the It.0 shortcut; split-double lands It.3.
        let proj = Mat4::perspective_infinite_reverse_rh(fov_y_rad, aspect, near_pc);
        proj * view
    }

    /// View basis vectors as f32 (for billboarding in the shader).
    pub fn basis_f32(&self) -> ([f32; 3], [f32; 3]) {
        (self.right().as_vec3().to_array(), self.up().as_vec3().to_array())
    }
}
