//! Physics joints, solved by `alice_physics` through [`PhysicsWorld::add_joint`] (requires the
//! `physics` feature). The engine keeps the description types; the constraint
//! law lives in `alice_physics`.
//!
//! A [`Joint`] is read in world space against the two bodies' poses at the
//! moment it is added, then held in the bodies' local frames (it turns with
//! them).
//!
//! ```rust
//! use alice_game_engine::joint::Joint;
//! use alice_game_engine::physics3d::{BodyDesc, PhysicsWorld};
//! use glam::Vec3;
//!
//! let mut world = PhysicsWorld::default();
//! let a = world.add_body(BodyDesc::fixed(Vec3::ZERO, 0.0));
//! let b = world.add_body(BodyDesc::dynamic(Vec3::new(5.0, 0.0, 0.0), 1.0, 0.0));
//! let joint = world.add_joint(Joint::distance(a, b, 5.0));
//! assert!(joint.is_some());
//! ```

use crate::math::Vec3;
use crate::physics3d::{BodyHandle, JointHandle, PhysicsWorld};

// ---------------------------------------------------------------------------
// Joint types
// ---------------------------------------------------------------------------

/// Joint constraint between two bodies.
#[derive(Debug, Clone)]
pub struct Joint {
    pub body_a: BodyHandle,
    pub body_b: BodyHandle,
    pub kind: JointKind,
    pub active: bool,
}

/// Joint variant.
#[derive(Debug, Clone)]
pub enum JointKind {
    /// Fixed distance between the two centres (a distance constraint).
    Distance { length: f32 },
    /// Rotation around a single axis: the pivot is body A's centre, body B
    /// keeps its distance from it and turns only about `axis` (world space
    /// at creation). The relative angle is limited to
    /// `[min_angle, max_angle]` unless the range is the full `[-π, π]`.
    Hinge {
        axis: Vec3,
        min_angle: f32,
        max_angle: f32,
    },
    /// Free rotation (3 DOF): the points `centre_a + anchor_a` and
    /// `centre_b + anchor_b` (world-space offsets at creation) are held
    /// together.
    Ball { anchor_a: Vec3, anchor_b: Vec3 },
    /// Spring between the two centres: the force along the line is
    /// `stiffness · (d - rest_length) + damping · ḋ` (N/m, N·s/m).
    Spring {
        rest_length: f32,
        stiffness: f32,
        damping: f32,
    },
    /// Prismatic (1-axis slide). Body B's centre is held on the line through
    /// body A's centre along `axis` (world space at creation, then fixed in
    /// A's frame); the offset along it is clamped to
    /// `[min_offset, max_offset]`.
    Slider {
        axis: Vec3,
        min_offset: f32,
        max_offset: f32,
    },
    /// Weld: holds `centre_b - centre_a == offset` (world space at creation,
    /// then turning with A) and locks the relative rotation.
    Fixed { offset: Vec3 },
    /// Cone-twist (humanoid joint). The pivot is body A's centre and body B
    /// keeps its distance from it. Body B may swing away from its pose at
    /// creation by at most `swing_half_angle` (measured on `twist_axis`,
    /// world space at creation) and twist about that axis by at most
    /// `twist_half_angle`.
    ConeTwist {
        twist_axis: Vec3,
        swing_half_angle: f32,
        twist_half_angle: f32,
    },
}

impl Joint {
    #[must_use]
    pub const fn distance(body_a: BodyHandle, body_b: BodyHandle, length: f32) -> Self {
        Self {
            body_a,
            body_b,
            kind: JointKind::Distance { length },
            active: true,
        }
    }

    #[must_use]
    pub fn hinge(body_a: BodyHandle, body_b: BodyHandle, axis: Vec3) -> Self {
        Self {
            body_a,
            body_b,
            kind: JointKind::Hinge {
                axis,
                min_angle: -std::f32::consts::PI,
                max_angle: std::f32::consts::PI,
            },
            active: true,
        }
    }

    #[must_use]
    pub const fn ball(
        body_a: BodyHandle,
        body_b: BodyHandle,
        anchor_a: Vec3,
        anchor_b: Vec3,
    ) -> Self {
        Self {
            body_a,
            body_b,
            kind: JointKind::Ball { anchor_a, anchor_b },
            active: true,
        }
    }

    #[must_use]
    pub const fn spring(
        body_a: BodyHandle,
        body_b: BodyHandle,
        rest_length: f32,
        stiffness: f32,
        damping: f32,
    ) -> Self {
        Self {
            body_a,
            body_b,
            kind: JointKind::Spring {
                rest_length,
                stiffness,
                damping,
            },
            active: true,
        }
    }

    /// Constructs a prismatic (1-axis slide) joint (see
    /// [`JointKind::Slider`]).
    #[must_use]
    pub const fn slider(
        body_a: BodyHandle,
        body_b: BodyHandle,
        axis: Vec3,
        min_offset: f32,
        max_offset: f32,
    ) -> Self {
        Self {
            body_a,
            body_b,
            kind: JointKind::Slider {
                axis,
                min_offset,
                max_offset,
            },
            active: true,
        }
    }

    /// Constructs a weld (fixed) joint that keeps `B - A == offset` (see
    /// [`JointKind::Fixed`]).
    #[must_use]
    pub const fn fixed(body_a: BodyHandle, body_b: BodyHandle, offset: Vec3) -> Self {
        Self {
            body_a,
            body_b,
            kind: JointKind::Fixed { offset },
            active: true,
        }
    }

    /// Constructs a cone-twist joint (humanoid hip / shoulder, see
    /// [`JointKind::ConeTwist`]).
    #[must_use]
    pub const fn cone_twist(
        body_a: BodyHandle,
        body_b: BodyHandle,
        twist_axis: Vec3,
        swing_half_angle: f32,
        twist_half_angle: f32,
    ) -> Self {
        Self {
            body_a,
            body_b,
            kind: JointKind::ConeTwist {
                twist_axis,
                swing_half_angle,
                twist_half_angle,
            },
            active: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Ragdoll builder
// ---------------------------------------------------------------------------

/// Ragdoll built by [`build_ragdoll`].
#[derive(Debug, Clone)]
pub struct RagdollDef {
    /// Bone name → body.
    pub bone_to_body: Vec<(String, BodyHandle)>,
    /// Joints created between consecutive bones.
    pub joints: Vec<JointHandle>,
}

/// Create one dynamic body per bone (5 kg, collision radius 0.1 m) and a
/// ball joint between each bone and the previous one. The joint's pivot is
/// the previous bone's position, so consecutive bones keep their distance.
pub fn build_ragdoll(skeleton_bones: &[(String, Vec3)], world: &mut PhysicsWorld) -> RagdollDef {
    let mut bone_to_body: Vec<(String, BodyHandle)> = Vec::with_capacity(skeleton_bones.len());
    let mut joints = Vec::new();
    let mut previous: Option<(BodyHandle, Vec3)> = None;
    for (name, pos) in skeleton_bones {
        let body = world.add_body(crate::physics3d::BodyDesc::dynamic(pos.0, 5.0, 0.1));
        if let Some((parent, parent_pos)) = previous {
            let joint = Joint::ball(parent, body, Vec3::ZERO, parent_pos - *pos);
            if let Some(handle) = world.add_joint(joint) {
                joints.push(handle);
            }
        }
        bone_to_body.push((name.clone(), body));
        previous = Some((body, *pos));
    }
    RagdollDef {
        bone_to_body,
        joints,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics3d::{BodyDesc, PhysicsConfig};
    use glam::Vec3 as G;

    fn world(gravity: G) -> PhysicsWorld {
        PhysicsWorld::new(PhysicsConfig {
            gravity,
            ..PhysicsConfig::default()
        })
    }

    fn v(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3::new(x, y, z)
    }

    fn run(w: &mut PhysicsWorld, steps: u32) {
        for _ in 0..steps {
            w.step_fixed();
        }
    }

    #[test]
    fn constructors_set_kind_and_active() {
        let mut w = world(G::ZERO);
        let a = w.add_body(BodyDesc::fixed(G::ZERO, 0.0));
        let b = w.add_body(BodyDesc::dynamic(G::X, 1.0, 0.0));
        assert!(matches!(
            Joint::hinge(a, b, Vec3::Y).kind,
            JointKind::Hinge { .. }
        ));
        assert!(Joint::distance(a, b, 1.0).active);
    }

    #[test]
    fn distance_joint_pulls_the_centres_to_the_length() {
        let mut w = world(G::ZERO);
        let a = w.add_body(BodyDesc::dynamic(G::ZERO, 1.0, 0.0));
        let b = w.add_body(BodyDesc::dynamic(G::new(10.0, 0.0, 0.0), 1.0, 0.0));
        assert!(w.add_joint(Joint::distance(a, b, 5.0)).is_some());
        run(&mut w, 30);
        let d = w.position(b).unwrap() - w.position(a).unwrap();
        assert!((d.length() - 5.0).abs() < 1e-3, "distance {}", d.length());
        // equal masses: the midpoint stays at x = 5
        let mid = (w.position(a).unwrap() + w.position(b).unwrap()) * 0.5;
        assert!((mid.x - 5.0).abs() < 1e-3);
    }

    #[test]
    fn inactive_joint_constrains_nothing() {
        let mut w = world(G::ZERO);
        let a = w.add_body(BodyDesc::dynamic(G::ZERO, 1.0, 0.0));
        let b = w.add_body(BodyDesc::dynamic(G::new(10.0, 0.0, 0.0), 1.0, 0.0));
        let mut j = Joint::distance(a, b, 5.0);
        j.active = false;
        let h = w.add_joint(j).unwrap();
        assert!(w.contains_joint(h));
        run(&mut w, 30);
        assert_eq!(w.position(b), Some(G::new(10.0, 0.0, 0.0)));
    }

    #[test]
    fn spring_oscillates_about_the_static_extension() {
        // Undamped: the time average of y over many periods is the equilibrium
        // -1 - m g / k = -1.0981 (period 2π/√(k/m) ≈ 0.63 s, 100 s averaged).
        let mut w = world(G::new(0.0, -9.81, 0.0));
        let a = w.add_body(BodyDesc::fixed(G::ZERO, 0.0));
        let b = w.add_body(BodyDesc::dynamic(G::new(0.0, -1.0, 0.0), 1.0, 0.0));
        w.add_joint(Joint::spring(a, b, 1.0, 100.0, 0.0)).unwrap();
        let steps = 6000;
        let mut sum = 0.0_f64;
        for _ in 0..steps {
            w.step_fixed();
            sum += f64::from(w.position(b).unwrap().y);
        }
        let mean = sum / f64::from(steps);
        assert!((mean + 1.0981).abs() < 2e-3, "mean y {mean}");
    }

    #[test]
    fn ball_joint_holds_the_anchors_together_under_gravity() {
        // pendulum: pivot at the static body, arm 2 m
        let mut w = world(G::new(0.0, -9.81, 0.0));
        let a = w.add_body(BodyDesc::fixed(G::ZERO, 0.0));
        let b = w.add_body(BodyDesc::dynamic(G::new(2.0, 0.0, 0.0), 1.0, 0.0));
        w.add_joint(Joint::ball(a, b, Vec3::ZERO, v(-2.0, 0.0, 0.0)))
            .unwrap();
        let mut lowest = 0.0_f32;
        for _ in 0..120 {
            w.step_fixed();
            let p = w.position(b).unwrap();
            assert!((p.length() - 2.0).abs() < 1e-2, "arm {p:?}");
            lowest = lowest.min(p.y);
        }
        // it passes the bottom of the circle
        assert!(lowest < -1.95, "lowest {lowest}");
    }

    #[test]
    fn hinge_keeps_the_body_in_the_plane_normal_to_the_axis() {
        let mut w = world(G::new(0.0, -9.81, -3.0));
        let a = w.add_body(BodyDesc::fixed(G::ZERO, 0.0));
        let b = w.add_body(BodyDesc::dynamic(G::new(1.5, 0.0, 0.0), 1.0, 0.0));
        w.add_joint(Joint::hinge(a, b, Vec3::Z)).unwrap();
        run(&mut w, 120);
        let p = w.position(b).unwrap();
        assert!(p.z.abs() < 1e-2, "out of plane {p:?}");
        assert!((p.length() - 1.5).abs() < 1e-2, "arm {p:?}");
        assert!(p.y < -0.5);
    }

    #[test]
    fn slider_keeps_the_body_on_the_axis_and_clamps_the_offset() {
        let mut w = world(G::new(0.0, -9.81, 0.0));
        let a = w.add_body(BodyDesc::fixed(G::ZERO, 0.0));
        let b = w.add_body(BodyDesc::dynamic(G::new(1.0, 0.0, 0.0), 1.0, 0.0));
        w.add_joint(Joint::slider(a, b, Vec3::X, 0.0, 2.0)).unwrap();
        w.set_velocity(b, G::new(5.0, 0.0, 0.0));
        run(&mut w, 120);
        let p = w.position(b).unwrap();
        assert!(p.y.abs() < 1e-2 && p.z.abs() < 1e-2, "off axis {p:?}");
        assert!(p.x <= 2.0 + 1e-2, "past the limit {p:?}");
    }

    #[test]
    fn fixed_joint_holds_the_offset_under_gravity() {
        let mut w = world(G::new(0.0, -9.81, 0.0));
        let a = w.add_body(BodyDesc::fixed(G::ZERO, 0.0));
        let b = w.add_body(BodyDesc::dynamic(G::new(1.0, 1.0, 0.0), 1.0, 0.0));
        w.add_joint(Joint::fixed(a, b, v(1.0, 1.0, 0.0))).unwrap();
        run(&mut w, 120);
        assert!((w.position(b).unwrap() - G::new(1.0, 1.0, 0.0)).length() < 1e-2);
    }

    #[test]
    fn cone_twist_limits_the_swing() {
        // hanging along -Y, pushed sideways: the swing stays within 30°
        let mut w = world(G::ZERO);
        let a = w.add_body(BodyDesc::fixed(G::ZERO, 0.0));
        let b = w.add_body(BodyDesc::dynamic(G::new(0.0, -1.0, 0.0), 1.0, 0.0));
        let swing = 30_f32.to_radians();
        w.add_joint(Joint::cone_twist(a, b, v(0.0, -1.0, 0.0), swing, 0.5))
            .unwrap();
        w.set_velocity(b, G::new(4.0, 0.0, 0.0));
        let mut widest = 0.0_f32;
        for _ in 0..120 {
            w.step_fixed();
            let p = w.position(b).unwrap();
            widest = widest.max(p.normalize().dot(G::NEG_Y).clamp(-1.0, 1.0).acos());
            assert!((p.length() - 1.0).abs() < 2e-2, "arm {p:?}");
        }
        assert!(
            widest <= swing + 0.05,
            "swing {} > {}",
            widest.to_degrees(),
            swing.to_degrees()
        );
        assert!(
            widest > swing * 0.5,
            "it did swing ({})",
            widest.to_degrees()
        );
    }

    #[test]
    fn ragdoll_has_one_body_per_bone_and_keeps_bone_lengths() {
        let mut w = world(G::new(0.0, -9.81, 0.0));
        let bones = vec![
            ("hip".to_string(), v(0.0, 1.0, 0.0)),
            ("knee".to_string(), v(0.0, 0.5, 0.0)),
            ("ankle".to_string(), v(0.0, 0.1, 0.0)),
        ];
        let doll = build_ragdoll(&bones, &mut w);
        assert_eq!(doll.bone_to_body.len(), 3);
        assert_eq!(doll.joints.len(), 2);
        assert_eq!(doll.bone_to_body[1].0, "knee");
        assert!(doll.joints.iter().all(|&j| w.contains_joint(j)));
        let body = |i: usize| doll.bone_to_body[i].1;
        w.apply_impulse(body(2), G::new(3.0, 0.0, 0.0));
        run(&mut w, 30);
        let d01 = (w.position(body(0)).unwrap() - w.position(body(1)).unwrap()).length();
        let d12 = (w.position(body(1)).unwrap() - w.position(body(2)).unwrap()).length();
        assert!((d01 - 0.5).abs() < 1e-2, "{d01}");
        assert!((d12 - 0.4).abs() < 1e-2, "{d12}");
    }
}
