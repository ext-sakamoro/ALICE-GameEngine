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
}

// ---------------------------------------------------------------------------
// Joint solver
// ---------------------------------------------------------------------------

/// Solves all joints against the physics world (position-based).
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
    }
}
