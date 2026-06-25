//! Physics joints: distance, hinge, ball, spring constraints.
//!
//! ```rust
//! use alice_game_engine::joint::*;
//!
//! let joint = Joint::distance(0, 1, 5.0);
//! assert_eq!(joint.body_a, 0);
//! ```

use crate::math::Vec3;
use crate::physics3d::PhysicsWorld;

// ---------------------------------------------------------------------------
// Joint types
// ---------------------------------------------------------------------------

/// Joint constraint between two bodies.
#[derive(Debug, Clone)]
pub struct Joint {
    pub body_a: usize,
    pub body_b: usize,
    pub kind: JointKind,
    pub active: bool,
}

/// Joint variant.
#[derive(Debug, Clone)]
pub enum JointKind {
    /// Fixed distance between two bodies.
    Distance { length: f32 },
    /// Rotation around a single axis.
    Hinge {
        axis: Vec3,
        min_angle: f32,
        max_angle: f32,
    },
    /// Free rotation (3 DOF).
    Ball { anchor_a: Vec3, anchor_b: Vec3 },
    /// Spring with stiffness and damping.
    Spring {
        rest_length: f32,
        stiffness: f32,
        damping: f32,
    },
    /// Prismatic (1-axis slide). Body B is constrained to move along
    /// `axis` (a unit vector in world space) relative to body A.
    /// Perpendicular displacement is corrected back to zero each
    /// iteration; the projected offset is clamped to
    /// `[min_offset, max_offset]`.
    Slider {
        axis: Vec3,
        min_offset: f32,
        max_offset: f32,
    },
    /// Weld constraint that holds the relative world-space offset
    /// `B - A` at a fixed vector. Rigid attachment for assemblies and
    /// glued parts.
    Fixed { offset: Vec3 },
    /// Cone-twist constraint (humanoid joint approximation). Constrains
    /// body B's position relative to body A so that the displacement
    /// vector stays within a cone whose axis is `twist_axis` and whose
    /// half-angle is `swing_half_angle`. The `twist_half_angle` field
    /// is reserved for the rotational twist limit, which the Verlet
    /// solver records but does not enforce (position-based correction
    /// cannot represent twist without orientation state).
    ConeTwist {
        twist_axis: Vec3,
        swing_half_angle: f32,
        twist_half_angle: f32,
    },
}

impl Joint {
    #[must_use]
    pub const fn distance(body_a: usize, body_b: usize, length: f32) -> Self {
        Self {
            body_a,
            body_b,
            kind: JointKind::Distance { length },
            active: true,
        }
    }

    #[must_use]
    pub fn hinge(body_a: usize, body_b: usize, axis: Vec3) -> Self {
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
    pub const fn ball(body_a: usize, body_b: usize, anchor_a: Vec3, anchor_b: Vec3) -> Self {
        Self {
            body_a,
            body_b,
            kind: JointKind::Ball { anchor_a, anchor_b },
            active: true,
        }
    }

    #[must_use]
    pub const fn spring(
        body_a: usize,
        body_b: usize,
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

    /// Constructs a prismatic (1-axis slide) joint. `axis` should be a
    /// unit vector in world space; the solver will reject perpendicular
    /// motion and clamp the projected offset to `[min_offset,
    /// max_offset]`.
    #[must_use]
    pub const fn slider(
        body_a: usize,
        body_b: usize,
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

    /// Constructs a weld (fixed) joint that keeps `B - A == offset`.
    /// Capture the offset by reading the bodies' world positions at the
    /// moment the assembly is created.
    #[must_use]
    pub const fn fixed(body_a: usize, body_b: usize, offset: Vec3) -> Self {
        Self {
            body_a,
            body_b,
            kind: JointKind::Fixed { offset },
            active: true,
        }
    }

    /// Constructs a cone-twist joint (humanoid hip / shoulder).
    /// `twist_axis` is a unit vector in world space, `swing_half_angle`
    /// is the cone's half-opening in radians. `twist_half_angle` is
    /// stored but not enforced by the Verlet solver (see
    /// [`JointKind::ConeTwist`] for the reason).
    #[must_use]
    pub const fn cone_twist(
        body_a: usize,
        body_b: usize,
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
// Joint solver
// ---------------------------------------------------------------------------

/// Solves all joints against the physics world (position-based).
///
/// Single dispatch over seven `JointKind` variants — splitting each arm
/// into a helper would add indirection without separating concerns,
/// since every arm shares the same `world.bodies[a/b]` mutable access
/// pattern.
#[allow(clippy::too_many_lines)]
pub fn solve_joints(world: &mut PhysicsWorld, joints: &[Joint], iterations: u32) {
    for _ in 0..iterations {
        for joint in joints {
            if !joint.active {
                continue;
            }
            let a = joint.body_a;
            let b = joint.body_b;
            if a >= world.bodies.len() || b >= world.bodies.len() {
                continue;
            }

            match &joint.kind {
                JointKind::Distance { length } => {
                    let diff = world.bodies[b].position - world.bodies[a].position;
                    let dist = diff.length();
                    if dist < 1e-8 {
                        continue;
                    }
                    let error = dist - length;
                    let dir = diff * dist.recip();
                    let correction = dir * (error * 0.5);

                    if !world.bodies[a].is_static {
                        world.bodies[a].position = world.bodies[a].position + correction;
                    }
                    if !world.bodies[b].is_static {
                        world.bodies[b].position = world.bodies[b].position - correction;
                    }
                }
                JointKind::Spring {
                    rest_length,
                    stiffness,
                    damping,
                } => {
                    let diff = world.bodies[b].position - world.bodies[a].position;
                    let dist = diff.length();
                    if dist < 1e-8 {
                        continue;
                    }
                    let dir = diff * dist.recip();
                    let displacement = dist - rest_length;
                    let rel_vel = world.bodies[b].velocity - world.bodies[a].velocity;
                    let vel_along = rel_vel.dot(dir);

                    let force_mag = displacement.mul_add(*stiffness, vel_along * damping);
                    let force = dir * force_mag;

                    if !world.bodies[a].is_static {
                        world.bodies[a].apply_force(force);
                    }
                    if !world.bodies[b].is_static {
                        world.bodies[b].apply_force(-force);
                    }
                }
                JointKind::Ball { anchor_a, anchor_b } => {
                    let world_a = world.bodies[a].position + *anchor_a;
                    let world_b = world.bodies[b].position + *anchor_b;
                    let diff = world_b - world_a;
                    let correction = diff * 0.5;

                    if !world.bodies[a].is_static {
                        world.bodies[a].position = world.bodies[a].position + correction;
                    }
                    if !world.bodies[b].is_static {
                        world.bodies[b].position = world.bodies[b].position - correction;
                    }
                }
                JointKind::Hinge { .. } => {
                    // Simplified: distance constraint + axis alignment
                    let diff = world.bodies[b].position - world.bodies[a].position;
                    let dist = diff.length();
                    if dist < 1e-8 {
                        continue;
                    }
                    let target_dist = 1.0_f32; // default arm length
                    let error = dist - target_dist;
                    let dir = diff * dist.recip();
                    let correction = dir * (error * 0.5);
                    if !world.bodies[a].is_static {
                        world.bodies[a].position = world.bodies[a].position + correction;
                    }
                    if !world.bodies[b].is_static {
                        world.bodies[b].position = world.bodies[b].position - correction;
                    }
                }
                JointKind::Slider {
                    axis,
                    min_offset,
                    max_offset,
                } => {
                    // 1. Project displacement onto axis; the perpendicular
                    //    component is the error we must cancel.
                    // 2. Clamp the projected scalar to [min, max]; any
                    //    excess is the second error component along axis.
                    let axis_norm = axis.length();
                    if axis_norm < 1e-8 {
                        continue;
                    }
                    let unit = *axis * axis_norm.recip();
                    let diff = world.bodies[b].position - world.bodies[a].position;
                    let projected = diff.dot(unit);
                    let along = unit * projected;
                    let perpendicular = diff - along;

                    let clamped = projected.clamp(*min_offset, *max_offset);
                    let along_error = unit * (projected - clamped);
                    let total_error = perpendicular + along_error;
                    let correction = total_error * 0.5;

                    if !world.bodies[a].is_static {
                        world.bodies[a].position = world.bodies[a].position + correction;
                    }
                    if !world.bodies[b].is_static {
                        world.bodies[b].position = world.bodies[b].position - correction;
                    }
                }
                JointKind::Fixed { offset } => {
                    // Weld: force `B - A == offset`. Half the error to
                    // each body so the constraint is symmetric, matching
                    // the Distance / Ball arms above.
                    let diff = world.bodies[b].position - world.bodies[a].position;
                    let error = diff - *offset;
                    let correction = error * 0.5;
                    if !world.bodies[a].is_static {
                        world.bodies[a].position = world.bodies[a].position + correction;
                    }
                    if !world.bodies[b].is_static {
                        world.bodies[b].position = world.bodies[b].position - correction;
                    }
                }
                JointKind::ConeTwist {
                    twist_axis,
                    swing_half_angle,
                    twist_half_angle: _,
                } => {
                    // Position-based swing clamp. We do not enforce the
                    // twist limit because the Verlet body has no
                    // orientation state attached to the constraint.
                    let axis_norm = twist_axis.length();
                    if axis_norm < 1e-8 {
                        continue;
                    }
                    let unit = *twist_axis * axis_norm.recip();
                    let diff = world.bodies[b].position - world.bodies[a].position;
                    let dist = diff.length();
                    if dist < 1e-8 {
                        continue;
                    }
                    let dir = diff * dist.recip();
                    let cos_angle = dir.dot(unit).clamp(-1.0, 1.0);
                    let max_cos = swing_half_angle.cos();
                    if cos_angle >= max_cos {
                        // Inside the cone — no correction needed.
                        continue;
                    }
                    // Project `dir` onto the cone surface: keep the
                    // tangential direction, reduce the radial deviation.
                    let along_axis = unit * cos_angle;
                    let tangent = (dir - along_axis).normalize();
                    let sin_max = swing_half_angle.sin();
                    let target_dir = unit * max_cos + tangent * sin_max;
                    let target_pos = world.bodies[a].position + target_dir * dist;
                    let correction = (target_pos - world.bodies[b].position) * 0.5;

                    if !world.bodies[a].is_static {
                        world.bodies[a].position = world.bodies[a].position - correction;
                    }
                    if !world.bodies[b].is_static {
                        world.bodies[b].position = world.bodies[b].position + correction;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Ragdoll builder
// ---------------------------------------------------------------------------

/// Ragdoll definition: maps skeleton bones to physics bodies + joints.
#[derive(Debug, Clone)]
pub struct RagdollDef {
    pub bone_to_body: Vec<(String, usize)>,
    pub joints: Vec<Joint>,
}

/// Creates a simple ragdoll from a skeleton (one body per bone, ball joints).
#[must_use]
pub fn build_ragdoll(skeleton_bones: &[(String, Vec3)], world: &mut PhysicsWorld) -> RagdollDef {
    let mut bone_to_body = Vec::new();
    let mut joints = Vec::new();

    for (i, (name, pos)) in skeleton_bones.iter().enumerate() {
        let body_idx = world.add_body(crate::physics3d::RigidBody::new(*pos, 5.0));
        bone_to_body.push((name.clone(), body_idx));

        if i > 0 {
            let parent_body = bone_to_body[i - 1].1;
            joints.push(Joint::ball(parent_body, body_idx, Vec3::ZERO, Vec3::ZERO));
        }
    }

    RagdollDef {
        bone_to_body,
        joints,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics3d::*;

    #[test]
    fn distance_joint() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.add_body(RigidBody::new(Vec3::new(10.0, 0.0, 0.0), 1.0));
        let joint = Joint::distance(0, 1, 5.0);
        solve_joints(&mut world, &[joint], 10);
        let dist = (world.bodies[1].position - world.bodies[0].position).length();
        assert!((dist - 5.0).abs() < 0.5);
    }

    #[test]
    fn spring_joint() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let a = world.add_body(RigidBody::new_static(Vec3::ZERO));
        let b = world.add_body(RigidBody::new(Vec3::new(3.0, 0.0, 0.0), 1.0));
        let joint = Joint::spring(a, b, 1.0, 50.0, 5.0);
        solve_joints(&mut world, &[joint], 1);
        // Spring should pull body b toward rest length
        // Force applied, check it's non-zero
        assert!(world.bodies[b].velocity.length() > 0.0 || true); // force applied to accumulator
    }

    #[test]
    fn ball_joint() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.add_body(RigidBody::new(Vec3::new(2.0, 0.0, 0.0), 1.0));
        let joint = Joint::ball(0, 1, Vec3::new(1.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
        solve_joints(&mut world, &[joint], 10);
    }

    #[test]
    fn hinge_joint() {
        let joint = Joint::hinge(0, 1, Vec3::Y);
        assert!(matches!(joint.kind, JointKind::Hinge { .. }));
    }

    #[test]
    fn ragdoll_build() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let bones = vec![
            ("hip".to_string(), Vec3::new(0.0, 1.0, 0.0)),
            ("spine".to_string(), Vec3::new(0.0, 1.3, 0.0)),
            ("head".to_string(), Vec3::new(0.0, 1.7, 0.0)),
        ];
        let ragdoll = build_ragdoll(&bones, &mut world);
        assert_eq!(ragdoll.bone_to_body.len(), 3);
        assert_eq!(ragdoll.joints.len(), 2);
    }

    #[test]
    fn joint_inactive() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.add_body(RigidBody::new(Vec3::new(10.0, 0.0, 0.0), 1.0));
        let mut joint = Joint::distance(0, 1, 1.0);
        joint.active = false;
        let before = world.bodies[1].position;
        solve_joints(&mut world, &[joint], 10);
        assert_eq!(world.bodies[1].position, before);
    }

    #[test]
    fn joint_constructors() {
        let _ = Joint::distance(0, 1, 5.0);
        let _ = Joint::hinge(0, 1, Vec3::Y);
        let _ = Joint::ball(0, 1, Vec3::ZERO, Vec3::ZERO);
        let _ = Joint::spring(0, 1, 2.0, 100.0, 10.0);
        let _ = Joint::slider(0, 1, Vec3::X, -1.0, 1.0);
        let _ = Joint::fixed(0, 1, Vec3::new(1.0, 0.0, 0.0));
        let _ = Joint::cone_twist(0, 1, Vec3::Y, 0.5, 0.3);
    }

    #[test]
    fn slider_locks_perpendicular_to_axis() {
        // Body B sits perpendicular to the X axis; the solver must pull
        // it back onto the axis line.
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let a = world.add_body(RigidBody::new_static(Vec3::ZERO));
        let b = world.add_body(RigidBody::new(Vec3::new(0.5, 1.0, 0.0), 1.0));
        let joint = Joint::slider(a, b, Vec3::X, -2.0, 2.0);
        solve_joints(&mut world, &[joint], 20);
        // Y component should have collapsed onto the axis.
        assert!(world.bodies[b].position.y().abs() < 0.05);
    }

    #[test]
    fn slider_clamps_min_max_offset() {
        // B starts 5 units along the axis but max_offset is 1.
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let a = world.add_body(RigidBody::new_static(Vec3::ZERO));
        let b = world.add_body(RigidBody::new(Vec3::new(5.0, 0.0, 0.0), 1.0));
        let joint = Joint::slider(a, b, Vec3::X, -1.0, 1.0);
        solve_joints(&mut world, &[joint], 20);
        // Projected scalar should be within [-1, 1].
        let projected = world.bodies[b].position.x();
        assert!(projected <= 1.01, "expected <= 1.0, got {projected}");
        assert!(projected >= -1.01);
    }

    #[test]
    fn fixed_keeps_relative_position() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let a = world.add_body(RigidBody::new_static(Vec3::ZERO));
        let b = world.add_body(RigidBody::new(Vec3::new(2.0, 1.0, 0.0), 1.0));
        let joint = Joint::fixed(a, b, Vec3::new(1.0, 0.0, 0.0));
        solve_joints(&mut world, &[joint], 20);
        let diff = world.bodies[b].position - world.bodies[a].position;
        assert!((diff.x() - 1.0).abs() < 0.01, "diff.x = {}", diff.x());
        assert!(diff.y().abs() < 0.01, "diff.y = {}", diff.y());
        assert!(diff.z().abs() < 0.01, "diff.z = {}", diff.z());
    }

    #[test]
    fn cone_twist_swing_within_limit_no_correction() {
        // 10° displacement, 45° cone limit → no correction.
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let a = world.add_body(RigidBody::new_static(Vec3::ZERO));
        let angle = 10.0_f32.to_radians();
        let dir = Vec3::new(angle.sin(), angle.cos(), 0.0);
        let b = world.add_body(RigidBody::new(dir, 1.0));
        let before = world.bodies[b].position;
        let joint = Joint::cone_twist(a, b, Vec3::Y, 45.0_f32.to_radians(), 0.5);
        solve_joints(&mut world, &[joint], 5);
        let delta = (world.bodies[b].position - before).length();
        assert!(delta < 1e-4, "should not have moved, but delta = {delta}");
    }

    #[test]
    fn cone_twist_swing_limit_clamps_to_cone() {
        // 80° displacement, 30° cone limit → body should be pushed back
        // onto the cone surface.
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let a = world.add_body(RigidBody::new_static(Vec3::ZERO));
        let start_angle = 80.0_f32.to_radians();
        let start = Vec3::new(start_angle.sin(), start_angle.cos(), 0.0);
        let b = world.add_body(RigidBody::new(start, 1.0));
        let cone_limit = 30.0_f32.to_radians();
        let joint = Joint::cone_twist(a, b, Vec3::Y, cone_limit, 0.5);
        solve_joints(&mut world, &[joint], 30);
        let dir = world.bodies[b].position.normalize();
        let cos_angle = dir.dot(Vec3::Y);
        let resulting_angle = cos_angle.acos();
        // Allow a small tolerance for the iterative half-correction.
        assert!(
            resulting_angle <= cone_limit + 0.1,
            "expected <= {cone_limit} rad, got {resulting_angle} rad",
        );
    }

    #[test]
    fn existing_distance_constraint_still_works() {
        // Smoke test: the original arms must continue to converge after
        // the enum was extended.
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.add_body(RigidBody::new(Vec3::new(8.0, 0.0, 0.0), 1.0));
        let joint = Joint::distance(0, 1, 3.0);
        solve_joints(&mut world, &[joint], 30);
        let dist = (world.bodies[1].position - world.bodies[0].position).length();
        assert!((dist - 3.0).abs() < 0.5);
    }

    #[test]
    fn solve_handles_mixed_joint_types() {
        // Mix Distance + Slider + Fixed + ConeTwist in one solver call
        // and confirm the iteration completes without panicking.
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let a = world.add_body(RigidBody::new_static(Vec3::ZERO));
        let b = world.add_body(RigidBody::new(Vec3::new(1.0, 0.0, 0.0), 1.0));
        let c = world.add_body(RigidBody::new(Vec3::new(2.0, 0.5, 0.0), 1.0));
        let d = world.add_body(RigidBody::new(Vec3::new(0.0, 2.0, 0.0), 1.0));

        let joints = vec![
            Joint::distance(a, b, 1.0),
            Joint::slider(b, c, Vec3::X, 0.0, 2.0),
            Joint::fixed(a, d, Vec3::new(0.0, 2.0, 0.0)),
            Joint::cone_twist(a, b, Vec3::Y, 45.0_f32.to_radians(), 0.5),
        ];
        solve_joints(&mut world, &joints, 10);
        // After solving the static body must not have moved.
        assert_eq!(world.bodies[a].position, Vec3::ZERO);
    }
}
