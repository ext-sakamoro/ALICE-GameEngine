//! 3D physics: rigid bodies, forces, broadphase AABB + narrowphase SDF.

use crate::math::{Quat, Vec3};
use crate::scene_graph::Aabb3;

// ---------------------------------------------------------------------------
// RigidBody
// ---------------------------------------------------------------------------

/// A 3D rigid body with mass, velocity, and angular velocity.
/// Uses Verlet integration (position-based) for stability.
#[derive(Debug, Clone)]
pub struct RigidBody {
    pub position: Vec3,
    pub prev_position: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    pub mass: f32,
    pub restitution: f32,
    pub friction: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub is_static: bool,
    pub sleeping: bool,
    pub sleep_threshold: f32,
    force_accumulator: Vec3,
    torque_accumulator: Vec3,
}

impl RigidBody {
    #[must_use]
    pub const fn new(position: Vec3, mass: f32) -> Self {
        Self {
            position,
            prev_position: position,
            rotation: Quat::IDENTITY,
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            mass,
            restitution: 0.3,
            friction: 0.5,
            linear_damping: 0.01,
            angular_damping: 0.05,
            is_static: false,
            sleeping: false,
            sleep_threshold: 0.01,
            force_accumulator: Vec3::ZERO,
            torque_accumulator: Vec3::ZERO,
        }
    }

    /// Creates a static (immovable) body.
    #[must_use]
    pub const fn new_static(position: Vec3) -> Self {
        Self {
            position,
            prev_position: position,
            rotation: Quat::IDENTITY,
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            mass: 0.0,
            restitution: 0.3,
            friction: 0.5,
            linear_damping: 0.0,
            angular_damping: 0.0,
            is_static: true,
            sleeping: false,
            sleep_threshold: 0.01,
            force_accumulator: Vec3::ZERO,
            torque_accumulator: Vec3::ZERO,
        }
    }

    /// Wakes the body if sleeping.
    pub const fn wake(&mut self) {
        self.sleeping = false;
    }

    /// Returns the inverse mass (0 for static bodies).
    #[inline]
    #[must_use]
    pub fn inv_mass(&self) -> f32 {
        if self.is_static || self.mass <= 0.0 {
            0.0
        } else {
            // Reciprocal multiply (division exorcism)
            self.mass.recip()
        }
    }

    /// Applies a force (accumulated over the frame).
    pub fn apply_force(&mut self, force: Vec3) {
        if !self.is_static {
            self.force_accumulator = self.force_accumulator + force;
        }
    }

    /// Applies an impulse (instant velocity change).
    pub fn apply_impulse(&mut self, impulse: Vec3) {
        if !self.is_static {
            self.velocity = self.velocity + impulse * self.inv_mass();
        }
    }

    /// Applies a torque.
    pub fn apply_torque(&mut self, torque: Vec3) {
        if !self.is_static {
            self.torque_accumulator = self.torque_accumulator + torque;
        }
    }

    /// Computes AABB for broadphase (simple sphere approximation).
    #[must_use]
    pub fn aabb(&self, half_extents: Vec3) -> Aabb3 {
        Aabb3::new(self.position - half_extents, self.position + half_extents)
    }

    /// Returns the kinetic energy.
    #[must_use]
    pub fn kinetic_energy(&self) -> f32 {
        0.5 * self.mass * self.velocity.length_squared()
    }
}

// ---------------------------------------------------------------------------
// PhysicsWorld
// ---------------------------------------------------------------------------

/// Gravity and integration for all rigid bodies.
pub struct PhysicsWorld {
    pub gravity: Vec3,
    pub bodies: Vec<RigidBody>,
    pub contacts: Vec<Contact3D>,
}

/// A 3D contact between two bodies.
#[derive(Debug, Clone, Copy)]
pub struct Contact3D {
    pub body_a: usize,
    pub body_b: usize,
    pub point: Vec3,
    pub normal: Vec3,
    pub penetration: f32,
}

impl PhysicsWorld {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            bodies: Vec::new(),
            contacts: Vec::new(),
        }
    }

    /// Adds a body and returns its index.
    pub fn add_body(&mut self, body: RigidBody) -> usize {
        self.bodies.push(body);
        self.bodies.len() - 1
    }

    /// Full physics step: integrate forces, detect collisions, resolve.
    pub fn step(&mut self, dt: f32) {
        self.step_with_half_extents(dt, Vec3::ONE);
    }

    /// Full physics step with configurable broadphase half-extents.
    /// Uses Stormer-Verlet integration for position stability.
    pub fn step_with_half_extents(&mut self, dt: f32, half_extents: Vec3) {
        let dt_sq = dt * dt;

        // 1. Verlet integration + force accumulation
        for body in &mut self.bodies {
            if body.is_static || body.sleeping {
                continue;
            }
            let gravity_force = Vec3::new(
                self.gravity.x() * body.mass,
                self.gravity.y() * body.mass,
                self.gravity.z() * body.mass,
            );
            body.force_accumulator = body.force_accumulator + gravity_force;
            let accel = body.force_accumulator * body.inv_mass();

            // Stormer-Verlet: x(t+dt) = 2*x(t) - x(t-dt) + a*dt^2
            let new_pos = body.position * 2.0 - body.prev_position + accel * dt_sq;

            // Damping
            let lin_damp = (1.0 - body.linear_damping).max(0.0);
            let damped_pos = body.position + (new_pos - body.position) * lin_damp;

            body.prev_position = body.position;
            body.position = damped_pos;

            // Derive velocity from position difference (for collision response)
            body.velocity = (body.position - body.prev_position) * dt.recip();

            // Angular: semi-implicit (Verlet for angular is complex)
            let inv_inertia = body.inv_mass() * 2.5;
            body.angular_velocity =
                body.angular_velocity + body.torque_accumulator * (inv_inertia * dt);
            let ang_damp = (1.0 - body.angular_damping).max(0.0);
            body.angular_velocity = body.angular_velocity * ang_damp;

            let half_dt = dt * 0.5;
            let w = body.angular_velocity;
            let dq = Quat::from_axis_angle(
                if w.length() > 1e-8 {
                    w.normalize()
                } else {
                    Vec3::Y
                },
                w.length() * half_dt,
            );
            body.rotation = (dq * body.rotation).normalize();

            body.force_accumulator = Vec3::ZERO;
            body.torque_accumulator = Vec3::ZERO;

            // Sleeping: put to sleep if energy is below threshold
            let energy = body.velocity.length_squared() + body.angular_velocity.length_squared();
            body.sleeping = energy < body.sleep_threshold * body.sleep_threshold;
        }

        // 2. Broadphase: sweep-and-prune on X axis
        let pairs = self.broadphase_sap(half_extents);

        // 3. Narrowphase + resolve
        self.contacts.clear();
        for (a, b) in &pairs {
            let diff = self.bodies[*b].position - self.bodies[*a].position;
            let dist = diff.length();
            let min_dist = half_extents.x() * 2.0;
            if dist < min_dist && dist > 1e-8 {
                let normal = diff * dist.recip();
                let penetration = min_dist - dist;
                self.contacts.push(Contact3D {
                    body_a: *a,
                    body_b: *b,
                    point: self.bodies[*a].position + normal * (dist * 0.5),
                    normal,
                    penetration,
                });
            }
        }

        // 4. Resolve contacts (iterate over index to avoid clone)
        for ci in 0..self.contacts.len() {
            let c = self.contacts[ci];
            self.resolve_contact(&c);
            // Wake colliding bodies
            self.bodies[c.body_a].sleeping = false;
            self.bodies[c.body_b].sleeping = false;
        }
    }

    /// Broadphase: sweep-and-prune on X axis. O(n log n) average.
    #[must_use]
    pub fn broadphase_sap(&self, half_extents: Vec3) -> Vec<(usize, usize)> {
        let len = self.bodies.len();
        if len < 2 {
            return Vec::new();
        }

        // Build sorted list of (x_min, body_index)
        let mut sorted: Vec<(f32, usize)> = (0..len)
            .map(|i| {
                let aabb = self.bodies[i].aabb(half_extents);
                (aabb.min.x(), i)
            })
            .collect();
        sorted.sort_by(|a, b| a.0.total_cmp(&b.0));

        let mut pairs = Vec::new();
        for si in 0..sorted.len() {
            let (_, i) = sorted[si];
            let aabb_a = self.bodies[i].aabb(half_extents);
            for &(x_min_j, j) in &sorted[si + 1..] {
                if x_min_j > aabb_a.max.x() {
                    break;
                }
                let aabb_b = self.bodies[j].aabb(half_extents);
                if aabb_a.intersects(&aabb_b) {
                    let key = if i < j { (i, j) } else { (j, i) };
                    pairs.push(key);
                }
            }
        }
        pairs
    }

    /// Legacy O(n^2) broadphase (kept for small body counts).
    #[must_use]
    pub fn broadphase(&self, half_extents: Vec3) -> Vec<(usize, usize)> {
        self.broadphase_sap(half_extents)
    }

    /// Resolves a contact with impulse-based response.
    pub fn resolve_contact(&mut self, contact: &Contact3D) {
        let inv_mass_a = self.bodies[contact.body_a].inv_mass();
        let inv_mass_b = self.bodies[contact.body_b].inv_mass();
        let inv_mass_sum = inv_mass_a + inv_mass_b;

        if inv_mass_sum < 1e-10 {
            return;
        }

        // Separate bodies
        let correction = contact.normal * (contact.penetration * inv_mass_sum.recip());
        self.bodies[contact.body_a].position =
            self.bodies[contact.body_a].position + correction * inv_mass_a;
        self.bodies[contact.body_b].position =
            self.bodies[contact.body_b].position - correction * inv_mass_b;

        // Compute relative velocity
        let rel_vel = self.bodies[contact.body_a].velocity - self.bodies[contact.body_b].velocity;
        let vel_along_normal = rel_vel.dot(contact.normal);

        if vel_along_normal > 0.0 {
            return; // separating
        }

        let e = self.bodies[contact.body_a]
            .restitution
            .min(self.bodies[contact.body_b].restitution);
        let j = -(1.0 + e) * vel_along_normal * inv_mass_sum.recip();
        let impulse = contact.normal * j;

        self.bodies[contact.body_a].velocity =
            self.bodies[contact.body_a].velocity + impulse * inv_mass_a;
        self.bodies[contact.body_b].velocity =
            self.bodies[contact.body_b].velocity - impulse * inv_mass_b;
    }

    /// Returns the number of bodies.
    #[must_use]
    pub const fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Returns the total kinetic energy of all bodies.
    #[must_use]
    pub fn total_kinetic_energy(&self) -> f32 {
        self.bodies.iter().map(RigidBody::kinetic_energy).sum()
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// CCD — Continuous Collision Detection
// ---------------------------------------------------------------------------

/// SDF-based continuous collision detection. Marches the body along its
/// velocity vector and tests the SDF at each step.
#[must_use]
pub fn sdf_ccd(
    sdf_eval: &dyn Fn(Vec3) -> f32,
    start: Vec3,
    velocity: Vec3,
    radius: f32,
    dt: f32,
    max_steps: u32,
) -> Option<CcdHit> {
    let dir = velocity * dt;
    let total_dist = dir.length();
    if total_dist < 1e-8 {
        return None;
    }
    let inv_total = total_dist.recip();
    let step_size = total_dist * (max_steps as f32).recip();
    let normalized = dir * inv_total;

    let mut t = 0.0_f32;
    for _ in 0..max_steps {
        let p = start + normalized * t;
        let d = sdf_eval(p);
        if d < radius {
            return Some(CcdHit {
                position: p,
                time_of_impact: t * inv_total,
                distance: d,
            });
        }
        // Sphere trace: advance by the safe distance
        t += d.max(step_size);
        if t > total_dist {
            return None;
        }
    }
    None
}

/// CCD hit result.
#[derive(Debug, Clone, Copy)]
pub struct CcdHit {
    pub position: Vec3,
    /// Fraction [0..1] of the velocity vector where contact occurred.
    pub time_of_impact: f32,
    pub distance: f32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rigidbody_new() {
        let rb = RigidBody::new(Vec3::ZERO, 10.0);
        assert_eq!(rb.mass, 10.0);
        assert!(!rb.is_static);
    }

    #[test]
    fn rigidbody_static() {
        let rb = RigidBody::new_static(Vec3::ZERO);
        assert!(rb.is_static);
        assert_eq!(rb.inv_mass(), 0.0);
    }

    #[test]
    fn rigidbody_inv_mass() {
        let rb = RigidBody::new(Vec3::ZERO, 4.0);
        assert!((rb.inv_mass() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn rigidbody_apply_impulse() {
        let mut rb = RigidBody::new(Vec3::ZERO, 2.0);
        rb.apply_impulse(Vec3::new(10.0, 0.0, 0.0));
        assert!((rb.velocity.x() - 5.0).abs() < 1e-6);
    }

    #[test]
    fn rigidbody_static_ignores_impulse() {
        let mut rb = RigidBody::new_static(Vec3::ZERO);
        rb.apply_impulse(Vec3::new(100.0, 0.0, 0.0));
        assert_eq!(rb.velocity.x(), 0.0);
    }

    #[test]
    fn rigidbody_aabb() {
        let rb = RigidBody::new(Vec3::new(5.0, 0.0, 0.0), 1.0);
        let aabb = rb.aabb(Vec3::ONE);
        assert!((aabb.min.x() - 4.0).abs() < 1e-6);
        assert!((aabb.max.x() - 6.0).abs() < 1e-6);
    }

    #[test]
    fn rigidbody_kinetic_energy() {
        let mut rb = RigidBody::new(Vec3::ZERO, 2.0);
        rb.velocity = Vec3::new(3.0, 4.0, 0.0);
        assert!((rb.kinetic_energy() - 25.0).abs() < 1e-4);
    }

    #[test]
    fn physics_world_step_gravity() {
        let mut world = PhysicsWorld::new();
        world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.step(1.0);
        assert!(world.bodies[0].velocity.y() < 0.0);
        assert!(world.bodies[0].position.y() < 0.0);
    }

    #[test]
    fn physics_world_static_doesnt_move() {
        let mut world = PhysicsWorld::new();
        world.add_body(RigidBody::new_static(Vec3::ZERO));
        world.step(1.0);
        assert_eq!(world.bodies[0].position.y(), 0.0);
    }

    #[test]
    fn physics_world_force() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let idx = world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.bodies[idx].linear_damping = 0.0;
        world.bodies[idx].apply_force(Vec3::new(10.0, 0.0, 0.0));
        world.step(1.0);
        assert!((world.bodies[idx].velocity.x() - 10.0).abs() < 1e-4);
    }

    #[test]
    fn physics_world_broadphase_overlap() {
        let mut world = PhysicsWorld::new();
        world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.add_body(RigidBody::new(Vec3::new(0.5, 0.0, 0.0), 1.0));
        world.add_body(RigidBody::new(Vec3::new(100.0, 0.0, 0.0), 1.0));
        let pairs = world.broadphase(Vec3::ONE);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], (0, 1));
    }

    #[test]
    fn physics_world_broadphase_no_overlap() {
        let mut world = PhysicsWorld::new();
        world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.add_body(RigidBody::new(Vec3::new(10.0, 0.0, 0.0), 1.0));
        let pairs = world.broadphase(Vec3::ONE);
        assert!(pairs.is_empty());
    }

    #[test]
    fn physics_resolve_contact() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let a = world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        let b = world.add_body(RigidBody::new(Vec3::new(1.5, 0.0, 0.0), 1.0));
        world.bodies[a].velocity = Vec3::new(5.0, 0.0, 0.0);
        world.bodies[b].velocity = Vec3::ZERO;

        // Normal points from B toward A (convention: pushes A away)
        let contact = Contact3D {
            body_a: a,
            body_b: b,
            point: Vec3::new(0.75, 0.0, 0.0),
            normal: Vec3::new(-1.0, 0.0, 0.0),
            penetration: 0.5,
        };
        world.resolve_contact(&contact);
        // Body A should slow down, B should speed up
        assert!(world.bodies[a].velocity.x() < 5.0);
        assert!(world.bodies[b].velocity.x() > 0.0);
    }

    #[test]
    fn physics_resolve_static_contact() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let a = world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        let b = world.add_body(RigidBody::new_static(Vec3::new(0.0, -1.0, 0.0)));
        world.bodies[a].velocity = Vec3::new(0.0, -5.0, 0.0);

        // Normal from B to A (upward, ground pushing body up)
        let contact = Contact3D {
            body_a: a,
            body_b: b,
            point: Vec3::new(0.0, -0.5, 0.0),
            normal: Vec3::new(0.0, 1.0, 0.0),
            penetration: 0.2,
        };
        world.resolve_contact(&contact);
        // vel_along_normal = (0,-5,0)·(0,1,0) = -5 < 0 → collision response
        assert!(world.bodies[a].velocity.y() > -5.0);
        // Static body should not move
        assert_eq!(world.bodies[b].velocity.y(), 0.0);
    }

    #[test]
    fn physics_world_body_count() {
        let mut world = PhysicsWorld::new();
        world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        assert_eq!(world.body_count(), 2);
    }

    #[test]
    fn physics_total_kinetic_energy() {
        let mut world = PhysicsWorld::new();
        let mut rb = RigidBody::new(Vec3::ZERO, 2.0);
        rb.velocity = Vec3::new(1.0, 0.0, 0.0);
        world.add_body(rb);
        assert!((world.total_kinetic_energy() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn physics_torque() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let idx = world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.bodies[idx].apply_torque(Vec3::new(0.0, 10.0, 0.0));
        world.step(1.0);
        assert!(world.bodies[idx].angular_velocity.y() > 0.0);
    }

    #[test]
    fn physics_rotation_integration() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let idx = world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.bodies[idx].angular_velocity = Vec3::new(0.0, 1.0, 0.0);
        world.step(0.1);
        assert_ne!(world.bodies[idx].rotation, Quat::IDENTITY);
    }

    #[test]
    fn physics_multiple_steps() {
        let mut world = PhysicsWorld::new();
        let idx = world.add_body(RigidBody::new(Vec3::new(0.0, 10.0, 0.0), 1.0));
        for _ in 0..100 {
            world.step(1.0 / 60.0);
        }
        assert!(world.bodies[idx].position.y() < 10.0);
    }

    #[test]
    fn physics_separating_bodies_no_impulse() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let a = world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        let b = world.add_body(RigidBody::new(Vec3::new(1.0, 0.0, 0.0), 1.0));
        // Bodies moving apart: A going left, B going right
        world.bodies[a].velocity = Vec3::new(-1.0, 0.0, 0.0);
        world.bodies[b].velocity = Vec3::new(1.0, 0.0, 0.0);

        // Normal points from B toward A (leftward)
        let contact = Contact3D {
            body_a: a,
            body_b: b,
            point: Vec3::new(0.5, 0.0, 0.0),
            normal: Vec3::new(-1.0, 0.0, 0.0),
            penetration: 0.1,
        };
        let vel_a_before = world.bodies[a].velocity.x();
        world.resolve_contact(&contact);
        // rel_vel = (-1 - 1) = -2, dot(-1,0,0) = 2 > 0 → separating, no impulse
        assert!((world.bodies[a].velocity.x() - vel_a_before).abs() < 1e-6);
    }

    #[test]
    fn physics_default() {
        let world = PhysicsWorld::default();
        assert_eq!(world.body_count(), 0);
        assert!((world.gravity.y() - (-9.81)).abs() < 1e-6);
    }

    #[test]
    fn physics_step_detects_collisions() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let a = world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        let b = world.add_body(RigidBody::new(Vec3::new(0.5, 0.0, 0.0), 1.0));
        world.bodies[a].velocity = Vec3::new(1.0, 0.0, 0.0);
        world.step(0.01);
        assert!(!world.contacts.is_empty());
    }

    #[test]
    fn physics_step_resolves_overlap() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.add_body(RigidBody::new(Vec3::new(0.5, 0.0, 0.0), 1.0));
        // Bodies overlap — step should push them apart
        let dist_before = (world.bodies[1].position - world.bodies[0].position).length();
        world.step(0.01);
        let dist_after = (world.bodies[1].position - world.bodies[0].position).length();
        assert!(dist_after >= dist_before);
    }

    #[test]
    fn physics_no_collision_far_apart() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.add_body(RigidBody::new(Vec3::new(100.0, 0.0, 0.0), 1.0));
        world.step(0.01);
        assert!(world.contacts.is_empty());
    }

    #[test]
    fn physics_ball_bounces_on_ground() {
        let mut world = PhysicsWorld::new();
        let ball = world.add_body(RigidBody::new(Vec3::new(0.0, 2.0, 0.0), 1.0));
        world.add_body(RigidBody::new_static(Vec3::new(0.0, 0.0, 0.0)));
        // Simulate 200 steps — ball should fall and bounce
        for _ in 0..200 {
            world.step(1.0 / 60.0);
        }
        // Ball should not have fallen through the ground
        assert!(world.bodies[ball].position.y() > -2.0);
    }

    #[test]
    fn physics_damping_slows_body() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let idx = world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.bodies[idx].velocity = Vec3::new(10.0, 0.0, 0.0);
        world.bodies[idx].linear_damping = 0.1;
        world.step(1.0 / 60.0);
        assert!(world.bodies[idx].velocity.x() < 10.0);
    }

    #[test]
    fn physics_sleeping_body_doesnt_move() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let idx = world.add_body(RigidBody::new(Vec3::new(5.0, 0.0, 0.0), 1.0));
        world.bodies[idx].sleeping = true;
        let pos_before = world.bodies[idx].position;
        world.step(1.0 / 60.0);
        assert_eq!(world.bodies[idx].position, pos_before);
    }

    #[test]
    fn physics_body_falls_asleep() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let idx = world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.bodies[idx].velocity = Vec3::new(0.001, 0.0, 0.0);
        world.bodies[idx].sleep_threshold = 0.1;
        world.step(1.0 / 60.0);
        assert!(world.bodies[idx].sleeping);
    }

    #[test]
    fn physics_collision_wakes_body() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let a = world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        let b = world.add_body(RigidBody::new(Vec3::new(0.5, 0.0, 0.0), 1.0));
        world.bodies[a].sleeping = true;
        world.bodies[b].sleeping = true;
        world.bodies[a].velocity = Vec3::new(1.0, 0.0, 0.0);
        world.bodies[a].sleeping = false;
        world.step(0.01);
        // Contact should wake both
        if !world.contacts.is_empty() {
            assert!(!world.bodies[b].sleeping);
        }
    }

    #[test]
    fn physics_angular_damping() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let idx = world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.bodies[idx].angular_velocity = Vec3::new(0.0, 10.0, 0.0);
        world.bodies[idx].angular_damping = 0.2;
        world.step(1.0 / 60.0);
        assert!(world.bodies[idx].angular_velocity.y() < 10.0);
    }

    #[test]
    fn physics_wake() {
        let mut rb = RigidBody::new(Vec3::ZERO, 1.0);
        rb.sleeping = true;
        rb.wake();
        assert!(!rb.sleeping);
    }

    #[test]
    fn physics_sap_broadphase() {
        let mut world = PhysicsWorld::new();
        world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.add_body(RigidBody::new(Vec3::new(0.5, 0.0, 0.0), 1.0));
        world.add_body(RigidBody::new(Vec3::new(100.0, 0.0, 0.0), 1.0));
        let pairs = world.broadphase_sap(Vec3::ONE);
        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn ccd_hits_sphere() {
        let sdf = |p: Vec3| p.length() - 1.0;
        let hit = sdf_ccd(
            &sdf,
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, -10.0),
            0.1,
            1.0,
            64,
        );
        assert!(hit.is_some());
        let h = hit.unwrap();
        assert!(h.time_of_impact > 0.0 && h.time_of_impact < 1.0);
    }

    #[test]
    fn ccd_misses() {
        let sdf = |p: Vec3| p.length() - 1.0;
        let hit = sdf_ccd(
            &sdf,
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 1.0, 0.0),
            0.1,
            1.0,
            64,
        );
        assert!(hit.is_none());
    }

    #[test]
    fn ccd_zero_velocity() {
        let sdf = |p: Vec3| p.length() - 1.0;
        let hit = sdf_ccd(&sdf, Vec3::new(5.0, 0.0, 0.0), Vec3::ZERO, 0.1, 1.0, 32);
        assert!(hit.is_none());
    }

    #[test]
    fn verlet_integration_stable() {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::ZERO;
        let idx = world.add_body(RigidBody::new(Vec3::ZERO, 1.0));
        world.bodies[idx].linear_damping = 0.0;
        world.bodies[idx].velocity = Vec3::new(1.0, 0.0, 0.0);
        // Set prev_position for consistent Verlet start
        world.bodies[idx].prev_position = Vec3::new(-1.0 / 60.0, 0.0, 0.0);
        for _ in 0..60 {
            world.step(1.0 / 60.0);
        }
        // Should have moved ~1 unit in 1 second
        assert!(world.bodies[idx].position.x() > 0.5);
    }
}
