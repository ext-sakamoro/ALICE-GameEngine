//! Camera controllers: FPS (first-person) and Orbit cameras.

use crate::math::{Quat, Vec3};

// ---------------------------------------------------------------------------
// FPSCamera
// ---------------------------------------------------------------------------

/// First-person camera with WASD movement and mouse look.
#[derive(Debug, Clone)]
pub struct FpsCamera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub move_speed: f32,
    pub look_sensitivity: f32,
    pub pitch_limit: f32,
}

impl FpsCamera {
    #[must_use]
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            move_speed: 5.0,
            look_sensitivity: 0.003,
            pitch_limit: std::f32::consts::FRAC_PI_2 - 0.01,
        }
    }

    /// Returns the forward direction vector.
    #[must_use]
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -(self.yaw.cos() * self.pitch.cos()),
        )
        .normalize()
    }

    /// Returns the right direction vector.
    #[must_use]
    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    /// Applies mouse delta to yaw/pitch.
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * self.look_sensitivity;
        self.pitch -= dy * self.look_sensitivity;
        self.pitch = self.pitch.clamp(-self.pitch_limit, self.pitch_limit);
    }

    /// Moves the camera in local space.
    /// `forward` = W/S axis (-1..1), `strafe` = A/D axis (-1..1), `up` = Q/E.
    pub fn move_local(&mut self, forward_input: f32, strafe_input: f32, up_input: f32, dt: f32) {
        let fwd = self.forward();
        let right = self.right();
        let speed = self.move_speed * dt;
        self.position = self.position + fwd * (forward_input * speed);
        self.position = self.position + right * (strafe_input * speed);
        self.position = self.position + Vec3::Y * (up_input * speed);
    }

    /// Returns the view matrix.
    #[must_use]
    pub fn view_matrix(&self) -> crate::math::Mat4 {
        let target = self.position + self.forward();
        crate::math::Mat4::look_at(self.position, target, Vec3::Y)
    }

    /// Returns the rotation quaternion.
    #[must_use]
    pub fn rotation(&self) -> Quat {
        let yaw_q = Quat::from_axis_angle(Vec3::Y, self.yaw);
        let pitch_q = Quat::from_axis_angle(Vec3::X, self.pitch);
        yaw_q * pitch_q
    }
}

impl Default for FpsCamera {
    fn default() -> Self {
        Self::new(Vec3::ZERO)
    }
}

// ---------------------------------------------------------------------------
// OrbitCamera
// ---------------------------------------------------------------------------

/// Orbit camera that revolves around a target point.
#[derive(Debug, Clone)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub pitch_limit: f32,
    pub orbit_sensitivity: f32,
    pub zoom_sensitivity: f32,
}

impl OrbitCamera {
    #[must_use]
    pub fn new(target: Vec3, distance: f32) -> Self {
        Self {
            target,
            distance,
            yaw: 0.0,
            pitch: 0.3,
            min_distance: 1.0,
            max_distance: 100.0,
            pitch_limit: std::f32::consts::FRAC_PI_2 - 0.05,
            orbit_sensitivity: 0.005,
            zoom_sensitivity: 0.5,
        }
    }

    /// Returns the camera position based on yaw, pitch, distance.
    #[must_use]
    pub fn position(&self) -> Vec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.sin();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.cos();
        self.target + Vec3::new(x, y, z)
    }

    /// Orbits the camera by the given mouse delta.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * self.orbit_sensitivity;
        self.pitch += dy * self.orbit_sensitivity;
        self.pitch = self.pitch.clamp(-self.pitch_limit, self.pitch_limit);
    }

    /// Zooms in/out.
    pub fn zoom(&mut self, delta: f32) {
        self.distance -= delta * self.zoom_sensitivity;
        self.distance = self.distance.clamp(self.min_distance, self.max_distance);
    }

    /// Pans the target point.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let right = Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin());
        let up = Vec3::Y;
        self.target = self.target + right * dx + up * dy;
    }

    /// Returns the view matrix.
    #[must_use]
    pub fn view_matrix(&self) -> crate::math::Mat4 {
        crate::math::Mat4::look_at(self.position(), self.target, Vec3::Y)
    }

    /// Returns the forward direction (toward target).
    #[must_use]
    pub fn forward(&self) -> Vec3 {
        (self.target - self.position()).normalize()
    }
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self::new(Vec3::ZERO, 10.0)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_camera_default() {
        let cam = FpsCamera::default();
        assert_eq!(cam.position, Vec3::ZERO);
        assert_eq!(cam.yaw, 0.0);
    }

    #[test]
    fn fps_camera_forward_initial() {
        let cam = FpsCamera::new(Vec3::ZERO);
        let fwd = cam.forward();
        // yaw=0, pitch=0 → forward is -Z
        assert!(fwd.z() < -0.9);
    }

    #[test]
    fn fps_camera_look() {
        let mut cam = FpsCamera::new(Vec3::ZERO);
        cam.look(100.0, 0.0);
        assert!(cam.yaw.abs() > 0.0);
    }

    #[test]
    fn fps_camera_pitch_clamped() {
        let mut cam = FpsCamera::new(Vec3::ZERO);
        cam.look(0.0, -10000.0);
        assert!(cam.pitch <= cam.pitch_limit);
        cam.look(0.0, 10000.0);
        assert!(cam.pitch >= -cam.pitch_limit);
    }

    #[test]
    fn fps_camera_move_forward() {
        let mut cam = FpsCamera::new(Vec3::ZERO);
        cam.move_local(1.0, 0.0, 0.0, 1.0);
        assert!(cam.position.z() < 0.0); // moved forward (-Z)
    }

    #[test]
    fn fps_camera_move_strafe() {
        let mut cam = FpsCamera::new(Vec3::ZERO);
        cam.move_local(0.0, 1.0, 0.0, 1.0);
        assert!(cam.position.x() > 0.0); // strafed right
    }

    #[test]
    fn fps_camera_move_up() {
        let mut cam = FpsCamera::new(Vec3::ZERO);
        cam.move_local(0.0, 0.0, 1.0, 1.0);
        assert!(cam.position.y() > 0.0);
    }

    #[test]
    fn fps_camera_view_matrix() {
        let cam = FpsCamera::new(Vec3::new(0.0, 0.0, 5.0));
        let m = cam.view_matrix();
        assert_ne!(m, crate::math::Mat4::IDENTITY);
    }

    #[test]
    fn fps_camera_right_perpendicular() {
        let cam = FpsCamera::new(Vec3::ZERO);
        let dot = cam.forward().dot(cam.right());
        assert!(dot.abs() < 1e-5);
    }

    #[test]
    fn orbit_camera_default() {
        let cam = OrbitCamera::default();
        assert_eq!(cam.distance, 10.0);
    }

    #[test]
    fn orbit_camera_position() {
        let cam = OrbitCamera::new(Vec3::ZERO, 10.0);
        let pos = cam.position();
        let dist = pos.distance(Vec3::ZERO);
        assert!((dist - 10.0).abs() < 1e-4);
    }

    #[test]
    fn orbit_camera_zoom() {
        let mut cam = OrbitCamera::new(Vec3::ZERO, 10.0);
        cam.zoom(5.0);
        assert!(cam.distance < 10.0);
    }

    #[test]
    fn orbit_camera_zoom_clamp() {
        let mut cam = OrbitCamera::new(Vec3::ZERO, 10.0);
        cam.zoom(1000.0);
        assert!(cam.distance >= cam.min_distance);
        cam.zoom(-10000.0);
        assert!(cam.distance <= cam.max_distance);
    }

    #[test]
    fn orbit_camera_orbit() {
        let mut cam = OrbitCamera::new(Vec3::ZERO, 10.0);
        cam.orbit(100.0, 0.0);
        assert!(cam.yaw.abs() > 0.0);
    }

    #[test]
    fn orbit_camera_pitch_clamped() {
        let mut cam = OrbitCamera::new(Vec3::ZERO, 10.0);
        cam.orbit(0.0, 10000.0);
        assert!(cam.pitch <= cam.pitch_limit);
    }

    #[test]
    fn orbit_camera_pan() {
        let mut cam = OrbitCamera::new(Vec3::ZERO, 10.0);
        cam.pan(1.0, 2.0);
        assert!(cam.target.y() > 0.0);
    }

    #[test]
    fn orbit_camera_view_matrix() {
        let cam = OrbitCamera::new(Vec3::ZERO, 5.0);
        let m = cam.view_matrix();
        assert_ne!(m, crate::math::Mat4::IDENTITY);
    }

    #[test]
    fn orbit_camera_forward_toward_target() {
        let cam = OrbitCamera::new(Vec3::new(0.0, 0.0, 0.0), 10.0);
        let fwd = cam.forward();
        let pos = cam.position();
        // Forward should point from position toward target
        let expected = (cam.target - pos).normalize();
        assert!((fwd.x() - expected.x()).abs() < 1e-4);
    }

    #[test]
    fn fps_camera_rotation() {
        let cam = FpsCamera::new(Vec3::ZERO);
        let q = cam.rotation();
        // Should be a valid quaternion
        let len = (q.0.x * q.0.x + q.0.y * q.0.y + q.0.z * q.0.z + q.0.w * q.0.w).sqrt();
        assert!((len - 1.0).abs() < 1e-5);
    }
}
