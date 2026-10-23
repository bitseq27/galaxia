//! Input → action mapping (PLAN It.0 key work; full mapping table §10.2 later).

use winit::event::{ElementState, KeyEvent, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    MoveForward(bool),
    MoveBack(bool),
    MoveLeft(bool),
    MoveRight(bool),
    MoveUp(bool),
    MoveDown(bool),
    SpeedUp,
    SpeedDown,
    ToggleOverlay,
    Quit,
}

/// Translate a winit window event into zero or more actions.
pub fn map_key_event(ev: &KeyEvent) -> Option<Action> {
    if ev.state != ElementState::Pressed && ev.state != ElementState::Released {
        return None;
    }
    let down = ev.state == ElementState::Pressed;
    let action = match ev.physical_key {
        PhysicalKey::Code(KeyCode::KeyW) => Action::MoveForward(down),
        PhysicalKey::Code(KeyCode::KeyS) => Action::MoveBack(down),
        PhysicalKey::Code(KeyCode::KeyA) => Action::MoveLeft(down),
        PhysicalKey::Code(KeyCode::KeyD) => Action::MoveRight(down),
        PhysicalKey::Code(KeyCode::Space) => Action::MoveUp(down),
        PhysicalKey::Code(KeyCode::ShiftLeft) | PhysicalKey::Code(KeyCode::ShiftRight) => {
            Action::MoveDown(down)
        }
        PhysicalKey::Code(KeyCode::Escape) | PhysicalKey::Code(KeyCode::KeyQ) => {
            if down {
                Action::Quit
            } else {
                return None;
            }
        }
        PhysicalKey::Code(KeyCode::Backquote)
            if down => {
                Action::ToggleOverlay
            }
        _ => return None,
    };
    Some(action)
}

pub fn map_scroll(delta: &MouseScrollDelta) -> Option<Action> {
    let lines = match delta {
        MouseScrollDelta::LineDelta(_, y) => *y,
        MouseScrollDelta::PixelDelta(pos) => (pos.y / 30.0) as f32,
    };
    if lines.abs() < 0.01 {
        return None;
    }
    Some(if lines > 0.0 {
        Action::SpeedUp
    } else {
        Action::SpeedDown
    })
}

/// Aggregate held-movement state into a unit-ish direction vector
/// (x = forward, y = right, z = up), consumed by Camera::update.
pub struct MoveState {
    pub fwd: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
}

impl MoveState {
    pub fn new() -> Self {
        Self { fwd: false, back: false, left: false, right: false, up: false, down: false }
    }

    pub fn apply(&mut self, a: Action) {
        match a {
            Action::MoveForward(v) => self.fwd = v,
            Action::MoveBack(v) => self.back = v,
            Action::MoveLeft(v) => self.left = v,
            Action::MoveRight(v) => self.right = v,
            Action::MoveUp(v) => self.up = v,
            Action::MoveDown(v) => self.down = v,
            _ => {}
        }
    }

    /// (forward, right, up) each in [-1, 1].
    pub fn axis(&self) -> glam::DVec3 {
        glam::DVec3::new(
            (self.fwd as i8 - self.back as i8) as f64,
            (self.right as i8 - self.left as i8) as f64,
            (self.up as i8 - self.down as i8) as f64,
        )
    }

    #[allow(dead_code)] // used by the governor/CI harness from It.6
    pub fn any_held(&self) -> bool {
        self.fwd || self.back || self.left || self.right || self.up || self.down
    }
}

/// Mouse-look sensitivity (radians per pixel of cursor motion).
pub const LOOK_SENS: f64 = 0.0025;

#[allow(dead_code)]
pub fn is_quit(a: Action) -> bool {
    a == Action::Quit
}

/// Scroll step for the speed exponent (decades of AU/s).
pub const SPEED_STEP: f32 = 0.15;

#[allow(dead_code)]
pub fn window_event_is_close(ev: &WindowEvent) -> bool {
    matches!(ev, WindowEvent::CloseRequested)
}
