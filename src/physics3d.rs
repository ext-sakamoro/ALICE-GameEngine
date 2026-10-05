//! 3D rigid-body physics, delegated to [`alice_physics`] (requires the
//! `physics` feature).
//!
//! The engine keeps no physics law of its own: integration, contacts,
//! joints, sleeping and queries all run in an `alice_physics::PhysicsWorld`
//! (deterministic 128-bit fixed point). This module is the adapter that
//! game code talks to in `glam` `f32` types.
//!
//! # Contract
//!
//! - **Handles**: [`BodyHandle`] / [`JointHandle`] carry an index and a
//!   generation. They stay valid across `remove_*` of *other* objects (the
//!   adapter follows `alice_physics`' swap-remove), and a removed handle
//!   never aliases a later object (its generation no longer matches, so
//!   every accessor returns `None` / `false`).
//! - **Units and axes**: metres, seconds, kilograms, radians; Y up. The
//!   `f32` ↔ fixed-point conversion happens only inside this module.
//! - **Time**: [`PhysicsWorld::update`] accumulates frame time and advances
//!   the inner world in whole steps of [`PhysicsConfig::fixed_dt`] (at most
//!   [`PhysicsConfig::max_steps_per_update`] per call; the remainder carries
//!   over). Results therefore do not depend on the frame rate.
//! - **Defaults**: gravity `(0, -9.81, 0)`; a body's `linear_damping` /
//!   `angular_damping` is the fraction of velocity lost per second
//!   (`0` = none), converted to the inner per-step retention factor; the
//!   inner world-wide damping is set to none.
//! - **Collision**: every body added with a positive `radius` collides
//!   (sphere of that radius); `radius = 0` adds a body that does not collide.
//! - **Contacts**: [`PhysicsWorld::contacts`] lists the contacts of the last
//!   step; `normal` points from `body_a` to `body_b`.
//! - **Forces**: [`PhysicsWorld::apply_force`] / [`PhysicsWorld::apply_torque`]
//!   accumulate and act over the next fixed step, then clear.
//! - **Material pairs**: restitution and friction of two bodies combine by
//!   their average (`alice_physics` default `CombineRule::Average`).
//! - **Sleeping**: a body at rest falls asleep within 60 fixed steps.
//! - **Defaults**: [`PhysicsConfig::default`] is gravity `(0, -9.81, 0)`,
//!   `fixed_dt = 1/60`, `substeps = 4`, `max_steps_per_update = 8`,
//!   `broadphase = Broadphase::DynamicTree`.
//! - **Degenerate input**: a `Dynamic` body with a mass that is not finite
//!   and positive is added as `Static`; a joint between a body and itself
//!   returns `None`; a negative, NaN or infinite `frame_dt` is ignored (not
//!   added to the accumulator, returns 0); when one `update` reaches
//!   `max_steps_per_update`, the time left over is dropped (no catch-up
//!   spiral).

use alice_physics::{Fix128, QuatFix, RigidBody, Vec3Fix};
use glam::{Quat, Vec3};

/// Stable reference to a body (index + generation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyHandle {
    index: u32,
    generation: u32,
}

/// Stable reference to a joint (index + generation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JointHandle {
    index: u32,
    generation: u32,
}

/// How a body moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    /// Moved by forces, contacts and joints.
    Dynamic,
    /// Never moves.
    Static,
    /// Moved only by [`PhysicsWorld::set_kinematic_target`].
    Kinematic,
}

impl BodyHandle {
    /// The handle packed into 64 bits (generation high, index low), for
    /// passing it through text protocols such as MCP.
    #[must_use]
    pub const fn to_bits(self) -> u64 {
        ((self.generation as u64) << 32) | self.index as u64
    }

    /// Inverse of [`BodyHandle::to_bits`]. A value that never came from
    /// `to_bits` is a handle like any stale one: accessors return `None`.
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self {
            index: bits as u32,
            generation: (bits >> 32) as u32,
        }
    }
}

/// Description of a body to add.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyDesc {
    /// Movement kind.
    pub kind: BodyKind,
    /// Initial position (m).
    pub position: Vec3,
    /// Initial orientation.
    pub rotation: Quat,
    /// Initial linear velocity (m/s).
    pub velocity: Vec3,
    /// Initial angular velocity (rad/s).
    pub angular_velocity: Vec3,
    /// Mass (kg), used for `Dynamic` bodies.
    pub mass: f32,
    /// Collision sphere radius (m); `0` = no collision.
    pub radius: f32,
    /// Coefficient of restitution.
    pub restitution: f32,
    /// Coulomb friction coefficient.
    pub friction: f32,
    /// Fraction of linear velocity lost per second.
    pub linear_damping: f32,
    /// Fraction of angular velocity lost per second.
    pub angular_damping: f32,
}

/// World settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsConfig {
    /// Gravity (m/s²).
    pub gravity: Vec3,
    /// Fixed step (s).
    pub fixed_dt: f32,
    /// Inner solver substeps per fixed step.
    pub substeps: u32,
    /// Upper bound on fixed steps run by one [`PhysicsWorld::update`].
    pub max_steps_per_update: u32,
    /// Broad-phase used to find candidate contact pairs.
    pub broadphase: Broadphase,
}

/// Broad-phase of the inner world. Both find the same candidate pairs, so a
/// world steps to the same result with either; they differ in cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Broadphase {
    /// Linear BVH rebuilt every step.
    Bvh,
    /// Persistent dynamic AABB tree: only bodies that leave their fattened
    /// box are re-inserted, cheaper when most bodies move little.
    #[default]
    DynamicTree,
}

/// One contact of the last step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact3D {
    /// First body.
    pub body_a: BodyHandle,
    /// Second body.
    pub body_b: BodyHandle,
    /// World-space contact point.
    pub point: Vec3,
    /// Unit normal from `body_a` to `body_b`.
    pub normal: Vec3,
    /// Penetration depth (m, ≥ 0).
    pub penetration: f32,
}

/// Sphere-cast result for an SDF (see [`sdf_ccd`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CcdHit {
    /// Sphere centre at impact.
    pub position: Vec3,
    /// Fraction of `dt` at impact, in `[0, 1]`.
    pub time_of_impact: f32,
    /// Signed distance at `position` (≈ radius at impact).
    pub distance: f32,
}

/// Rigid-body world backed by `alice_physics`.
pub struct PhysicsWorld {
    config: PhysicsConfig,
    inner: alice_physics::PhysicsWorld,
    /// `inner.step` argument (`config.fixed_dt` in fixed point).
    fixed_dt_fix: Fix128,
    /// Body slots, indexed by `BodyHandle::index`.
    bodies: Vec<BodySlot>,
    free_bodies: Vec<u32>,
    /// Slot of each inner body (parallel to `inner.bodies`).
    inner_to_body: Vec<u32>,
    /// Joint slots, indexed by `JointHandle::index`.
    joints: Vec<JointSlot>,
    free_joints: Vec<u32>,
    /// Slot of each inner joint (parallel to `inner.joints`).
    joint_to_slot: Vec<u32>,
    /// Slot of each inner distance constraint (parallel to
    /// `inner.distance_constraints`).
    distance_to_slot: Vec<u32>,
    /// Frame time not yet consumed by fixed steps (s).
    accumulator: f64,
    contacts: Vec<Contact3D>,
}

#[derive(Clone, Copy, Debug)]
struct BodySlot {
    generation: u32,
    /// Inner index while live.
    inner: Option<usize>,
    kind: BodyKind,
    radius: f32,
    force: Vec3,
    torque: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InnerJoint {
    /// Index into `inner.joints`.
    Joint(usize),
    /// Index into `inner.distance_constraints`.
    Distance(usize),
    /// `Joint::active == false`: recorded, not solved.
    Inactive,
}

#[derive(Clone, Copy, Debug)]
struct LiveJoint {
    inner: InnerJoint,
    body_a: u32,
    body_b: u32,
}

#[derive(Clone, Copy, Debug)]
struct JointSlot {
    generation: u32,
    live: Option<LiveJoint>,
}

// ---------------------------------------------------------------------------
// f32 <-> fixed point (the only place the engine converts)
// ---------------------------------------------------------------------------

pub(crate) fn fx(v: f32) -> Fix128 {
    Fix128::from_f64(f64::from(v))
}

pub(crate) fn to_fix(v: Vec3) -> Vec3Fix {
    Vec3Fix::new(fx(v.x), fx(v.y), fx(v.z))
}

pub(crate) fn from_fix(v: Vec3Fix) -> Vec3 {
    Vec3::new(v.x.to_f32(), v.y.to_f32(), v.z.to_f32())
}

fn quat_to_fix(q: Quat) -> QuatFix {
    QuatFix::new(fx(q.x), fx(q.y), fx(q.z), fx(q.w))
}

fn quat_from_fix(q: QuatFix) -> Quat {
    Quat::from_xyzw(q.x.to_f32(), q.y.to_f32(), q.z.to_f32(), q.w.to_f32())
}

/// Per-step velocity retention for a body losing `lost_per_second` of its
/// velocity per second: `(1 - lost)^dt`, so `1 / dt` steps retain `1 - lost`.
fn retention_per_step(lost_per_second: f32, fixed_dt: f32) -> Fix128 {
    if lost_per_second.is_nan() || lost_per_second <= 0.0 {
        return Fix128::ONE;
    }
    let kept = 1.0 - f64::from(lost_per_second.min(1.0));
    Fix128::from_f64(kept.powf(f64::from(fixed_dt)))
}

/// An engine joint description converted to the inner world.
enum InnerJointDesc {
    Joint(Box<alice_physics::Joint>),
    Distance(alice_physics::DistanceConstraint),
}

// Moved once per `add_joint` into the box that keeps the enum small.
#[allow(clippy::large_types_passed_by_value)]
fn boxed(joint: alice_physics::Joint) -> InnerJointDesc {
    InnerJointDesc::Joint(Box::new(joint))
}

/// Convert a world-space [`crate::joint::JointKind`] to an inner joint
/// between inner bodies `a` / `b`, using their current poses (see the
/// `JointKind` docs for each mapping).
fn joint_to_inner(
    kind: &crate::joint::JointKind,
    body_a: &RigidBody,
    body_b: &RigidBody,
    a: usize,
    b: usize,
) -> InnerJointDesc {
    use crate::joint::JointKind;
    use alice_physics::{
        BallJoint, ConeTwistJoint, DistanceConstraint, FixedJoint, HingeJoint, Joint, SliderJoint,
        SpringJoint,
    };
    let zero = Vec3Fix::ZERO;
    let in_a = |v: Vec3| body_a.rotation.conjugate().rotate_vec(to_fix(v));
    let in_b = |v: Vec3| body_b.rotation.conjugate().rotate_vec(to_fix(v));
    // Body A's centre in body B's frame (the pivot of hinge / cone-twist).
    let pivot_in_b = body_b
        .rotation
        .conjugate()
        .rotate_vec(body_a.position - body_b.position);
    match *kind {
        JointKind::Distance { length } => {
            InnerJointDesc::Distance(DistanceConstraint::new(a, b, zero, zero, fx(length)))
        }
        JointKind::Hinge {
            axis,
            min_angle,
            max_angle,
        } => {
            let axis = axis.0.normalize_or_zero();
            let mut j = HingeJoint::new(a, b, zero, pivot_in_b, in_a(axis), in_b(axis));
            if min_angle > -std::f32::consts::PI || max_angle < std::f32::consts::PI {
                j = j.with_limits(fx(min_angle), fx(max_angle));
            }
            boxed(Joint::Hinge(j))
        }
        JointKind::Ball { anchor_a, anchor_b } => boxed(Joint::Ball(BallJoint::new(
            a,
            b,
            in_a(anchor_a.0),
            in_b(anchor_b.0),
        ))),
        JointKind::Spring {
            rest_length,
            stiffness,
            damping,
        } => boxed(Joint::Spring(SpringJoint::new(
            a,
            b,
            zero,
            zero,
            fx(rest_length),
            fx(stiffness),
            fx(damping),
        ))),
        JointKind::Slider {
            axis,
            min_offset,
            max_offset,
        } => boxed(Joint::Slider(
            SliderJoint::new(a, b, in_a(axis.0.normalize_or_zero()), zero, zero)
                .with_limits(fx(min_offset), fx(max_offset)),
        )),
        JointKind::Fixed { offset } => boxed(Joint::Fixed(FixedJoint::new(
            a,
            b,
            in_a(offset.0),
            zero,
            body_a.rotation.conjugate().mul(body_b.rotation),
        ))),
        JointKind::ConeTwist {
            twist_axis,
            swing_half_angle,
            twist_half_angle,
        } => {
            let axis = twist_axis.0.normalize_or_zero();
            boxed(Joint::ConeTwist(
                ConeTwistJoint::new(a, b, zero, pivot_in_b, in_a(axis), in_b(axis))
                    .with_limits(fx(swing_half_angle), fx(twist_half_angle)),
            ))
        }
    }
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            fixed_dt: 1.0 / 60.0,
            substeps: 4,
            max_steps_per_update: 8,
            broadphase: Broadphase::DynamicTree,
        }
    }
}

impl BodyDesc {
    /// Dynamic sphere body at `position` with `mass` and collision `radius`
    /// (restitution 0.3, friction 0.5, no damping).
    #[must_use]
    pub const fn dynamic(position: Vec3, mass: f32, radius: f32) -> Self {
        Self {
            kind: BodyKind::Dynamic,
            position,
            rotation: Quat::IDENTITY,
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            mass,
            radius,
            restitution: 0.3,
            friction: 0.5,
            linear_damping: 0.0,
            angular_damping: 0.0,
        }
    }

    /// Static sphere body at `position` with collision `radius`.
    #[must_use]
    pub const fn fixed(position: Vec3, radius: f32) -> Self {
        Self {
            kind: BodyKind::Static,
            mass: 0.0,
            ..Self::dynamic(position, 0.0, radius)
        }
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new(PhysicsConfig::default())
    }
}

impl std::fmt::Debug for PhysicsWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysicsWorld")
            .field("config", &self.config)
            .field("bodies", &self.body_count())
            .field("joints", &self.joint_count())
            .finish_non_exhaustive()
    }
}

impl PhysicsWorld {
    /// New empty world.
    ///
    /// # Panics
    ///
    /// If `config.fixed_dt` is not a positive finite number or
    /// `config.substeps` is 0.
    #[must_use]
    pub fn new(config: PhysicsConfig) -> Self {
        assert!(
            config.fixed_dt.is_finite() && config.fixed_dt > 0.0,
            "PhysicsConfig::fixed_dt must be positive and finite, got {}",
            config.fixed_dt
        );
        assert!(
            config.substeps > 0,
            "PhysicsConfig::substeps must be at least 1"
        );
        let solver = alice_physics::PhysicsConfig {
            substeps: config.substeps as usize,
            gravity: to_fix(config.gravity),
            // Per-body damping carries the engine's per-second loss (contract).
            damping: Fix128::ONE,
            ..alice_physics::PhysicsConfig::default()
        };
        Self {
            config,
            inner: {
                let mut inner = alice_physics::PhysicsWorld::new(solver);
                inner.set_broadphase(match config.broadphase {
                    Broadphase::Bvh => alice_physics::solver::Broadphase::Bvh,
                    Broadphase::DynamicTree => alice_physics::solver::Broadphase::DynamicTree,
                });
                inner
            },
            fixed_dt_fix: Fix128::from_f64(f64::from(config.fixed_dt)),
            bodies: Vec::new(),
            free_bodies: Vec::new(),
            inner_to_body: Vec::new(),
            joints: Vec::new(),
            free_joints: Vec::new(),
            joint_to_slot: Vec::new(),
            distance_to_slot: Vec::new(),
            accumulator: 0.0,
            contacts: Vec::new(),
        }
    }

    /// Current settings.
    #[must_use]
    pub const fn config(&self) -> PhysicsConfig {
        self.config
    }

    fn live_slot(&self, body: BodyHandle) -> Option<&BodySlot> {
        self.bodies
            .get(body.index as usize)
            .filter(|s| s.generation == body.generation && s.inner.is_some())
    }

    fn resolve(&self, body: BodyHandle) -> Option<usize> {
        self.live_slot(body).and_then(|s| s.inner)
    }

    /// Add a body.
    ///
    /// A dynamic body with `radius > 0` gets the inertia of a solid sphere
    /// of that radius (`2/5 m r²`); with `radius = 0` the inner default
    /// (unit sphere). A dynamic body whose mass is not finite and positive
    /// is added as `Static` (module contract).
    pub fn add_body(&mut self, desc: BodyDesc) -> BodyHandle {
        let mut desc = desc;
        if desc.kind == BodyKind::Dynamic && !(desc.mass.is_finite() && desc.mass > 0.0) {
            desc.kind = BodyKind::Static;
        }
        let position = to_fix(desc.position);
        let mut body = match desc.kind {
            BodyKind::Dynamic => RigidBody::new_dynamic(position, fx(desc.mass)),
            BodyKind::Static => RigidBody::new_static(position),
            BodyKind::Kinematic => RigidBody::new_kinematic(position),
        };
        body.set_rotation(quat_to_fix(desc.rotation.normalize()));
        if desc.kind != BodyKind::Static {
            body.velocity = to_fix(desc.velocity);
            body.angular_velocity = to_fix(desc.angular_velocity);
        }
        body.restitution = fx(desc.restitution);
        body.friction = fx(desc.friction);
        body.linear_damping = retention_per_step(desc.linear_damping, self.config.fixed_dt);
        body.angular_damping = retention_per_step(desc.angular_damping, self.config.fixed_dt);
        if desc.kind == BodyKind::Dynamic && desc.radius > 0.0 {
            // Solid-sphere inertia from alice_physics at unit density, scaled
            // to the body's mass (inertia is linear in the mass).
            let unit = alice_physics::sphere_mass_properties(fx(desc.radius), Fix128::ONE);
            let inertia = unit.inertia_tensor.col0.x * (fx(desc.mass) / unit.mass);
            let inv = Fix128::ONE / inertia;
            body.inv_inertia = Vec3Fix::new(inv, inv, inv);
        }

        let radius = if desc.radius > 0.0 { desc.radius } else { 0.0 };
        let inner = if radius > 0.0 {
            self.inner.add_body_with_radius(body, fx(radius))
        } else {
            self.inner.add_body(body)
        };

        let slot = BodySlot {
            generation: 0,
            inner: Some(inner),
            kind: desc.kind,
            radius,
            force: Vec3::ZERO,
            torque: Vec3::ZERO,
        };
        let index = if let Some(index) = self.free_bodies.pop() {
            let generation = self.bodies[index as usize].generation;
            self.bodies[index as usize] = BodySlot { generation, ..slot };
            index
        } else {
            self.bodies.push(slot);
            u32::try_from(self.bodies.len() - 1).expect("more than u32::MAX bodies")
        };
        self.inner_to_body.push(index);
        debug_assert_eq!(self.inner_to_body.len(), self.inner.bodies.len());
        BodyHandle {
            index,
            generation: self.bodies[index as usize].generation,
        }
    }

    /// Remove a body (and the joints attached to it). `false` if the handle
    /// is stale.
    pub fn remove_body(&mut self, body: BodyHandle) -> bool {
        let Some(inner) = self.resolve(body) else {
            return false;
        };
        let slot = body.index;

        // Joints on this body: the inner world drops them with an
        // order-preserving `retain`; mirror it on the slot tables.
        let touches = |live: &LiveJoint| live.body_a == slot || live.body_b == slot;
        for j in 0..self.joints.len() {
            if self.joints[j].live.as_ref().is_some_and(touches) {
                self.kill_joint_slot(j);
            }
        }
        let joints = &self.joints;
        self.joint_to_slot
            .retain(|&s| joints[s as usize].live.is_some());
        self.distance_to_slot
            .retain(|&s| joints[s as usize].live.is_some());

        let removed = self.inner.remove_body(inner);
        debug_assert!(removed.is_some());

        // Follow the inner swap-remove.
        self.inner_to_body.swap_remove(inner);
        if let Some(&moved) = self.inner_to_body.get(inner) {
            self.bodies[moved as usize].inner = Some(inner);
        }
        self.reindex_joints();

        let s = &mut self.bodies[slot as usize];
        s.inner = None;
        s.generation = s.generation.wrapping_add(1);
        self.free_bodies.push(slot);
        debug_assert_eq!(self.inner.joints.len(), self.joint_to_slot.len());
        debug_assert_eq!(
            self.inner.distance_constraints.len(),
            self.distance_to_slot.len()
        );
        true
    }

    fn kill_joint_slot(&mut self, j: usize) {
        let s = &mut self.joints[j];
        s.live = None;
        s.generation = s.generation.wrapping_add(1);
        self.free_joints
            .push(u32::try_from(j).expect("more than u32::MAX joints"));
    }

    /// Re-derive each live joint's inner index from the parallel tables.
    fn reindex_joints(&mut self) {
        for (k, &s) in self.joint_to_slot.iter().enumerate() {
            if let Some(live) = self.joints[s as usize].live.as_mut() {
                live.inner = InnerJoint::Joint(k);
            }
        }
        for (k, &s) in self.distance_to_slot.iter().enumerate() {
            if let Some(live) = self.joints[s as usize].live.as_mut() {
                live.inner = InnerJoint::Distance(k);
            }
        }
    }

    /// Whether the handle refers to a live body.
    #[must_use]
    pub fn contains(&self, body: BodyHandle) -> bool {
        self.resolve(body).is_some()
    }

    /// Number of live bodies.
    #[must_use]
    pub fn body_count(&self) -> usize {
        self.inner.body_count()
    }

    /// Number of live joints (active and inactive).
    #[must_use]
    pub fn joint_count(&self) -> usize {
        self.joints.iter().filter(|j| j.live.is_some()).count()
    }

    /// Whether the handle refers to a live joint.
    #[must_use]
    pub fn contains_joint(&self, joint: JointHandle) -> bool {
        self.joints
            .get(joint.index as usize)
            .is_some_and(|s| s.generation == joint.generation && s.live.is_some())
    }

    fn inner_body(&self, body: BodyHandle) -> Option<&RigidBody> {
        self.resolve(body).map(|i| &self.inner.bodies[i])
    }

    fn inner_body_mut(&mut self, body: BodyHandle) -> Option<&mut RigidBody> {
        self.resolve(body).map(|i| &mut self.inner.bodies[i])
    }

    /// Position, or `None` for a stale handle.
    #[must_use]
    pub fn position(&self, body: BodyHandle) -> Option<Vec3> {
        self.inner_body(body).map(|b| from_fix(b.position))
    }

    /// Orientation, or `None` for a stale handle.
    #[must_use]
    pub fn rotation(&self, body: BodyHandle) -> Option<Quat> {
        self.inner_body(body).map(|b| quat_from_fix(b.rotation))
    }

    /// Linear velocity, or `None` for a stale handle.
    #[must_use]
    pub fn velocity(&self, body: BodyHandle) -> Option<Vec3> {
        self.inner_body(body).map(|b| from_fix(b.velocity))
    }

    /// Angular velocity, or `None` for a stale handle.
    #[must_use]
    pub fn angular_velocity(&self, body: BodyHandle) -> Option<Vec3> {
        self.inner_body(body).map(|b| from_fix(b.angular_velocity))
    }

    /// Collision radius (`0` = no collision), or `None` for a stale handle.
    #[must_use]
    pub fn radius(&self, body: BodyHandle) -> Option<f32> {
        self.live_slot(body).map(|s| s.radius)
    }

    /// Movement kind, or `None` for a stale handle.
    #[must_use]
    pub fn kind(&self, body: BodyHandle) -> Option<BodyKind> {
        self.live_slot(body).map(|s| s.kind)
    }

    fn wake_inner(&mut self, inner: usize) {
        if self.inner.is_sleeping(inner) {
            self.inner.wake_body(inner);
        }
    }

    /// Teleport a body (velocity unchanged). `false` if stale.
    pub fn set_position(&mut self, body: BodyHandle, position: Vec3) -> bool {
        let Some(i) = self.resolve(body) else {
            return false;
        };
        self.inner.bodies[i].set_position(to_fix(position));
        self.wake_inner(i);
        true
    }

    /// Set the linear velocity. `false` if stale.
    pub fn set_velocity(&mut self, body: BodyHandle, velocity: Vec3) -> bool {
        let Some(i) = self.resolve(body) else {
            return false;
        };
        self.inner.bodies[i].set_velocity(to_fix(velocity));
        self.wake_inner(i);
        true
    }

    /// Drive a kinematic body to `position` / `rotation` over the next fixed
    /// step. `false` if stale or not kinematic.
    pub fn set_kinematic_target(
        &mut self,
        body: BodyHandle,
        position: Vec3,
        rotation: Quat,
    ) -> bool {
        if self.live_slot(body).map(|s| s.kind) != Some(BodyKind::Kinematic) {
            return false;
        }
        let rotation = quat_to_fix(rotation.normalize());
        self.inner_body_mut(body)
            .map(|b| b.set_kinematic_target(to_fix(position), rotation))
            .is_some()
    }

    /// Instant velocity change `Δv = impulse / m` at the centre of mass.
    pub fn apply_impulse(&mut self, body: BodyHandle, impulse: Vec3) -> bool {
        let Some(i) = self.resolve(body) else {
            return false;
        };
        self.inner.bodies[i].apply_impulse(to_fix(impulse));
        self.wake_inner(i);
        true
    }

    /// Force acting over the next fixed step.
    pub fn apply_force(&mut self, body: BodyHandle, force: Vec3) -> bool {
        let Some(i) = self.resolve(body) else {
            return false;
        };
        self.bodies[body.index as usize].force += force;
        self.wake_inner(i);
        true
    }

    /// Torque acting over the next fixed step.
    pub fn apply_torque(&mut self, body: BodyHandle, torque: Vec3) -> bool {
        let Some(i) = self.resolve(body) else {
            return false;
        };
        self.bodies[body.index as usize].torque += torque;
        self.wake_inner(i);
        true
    }

    /// Wake a sleeping body. `false` if stale.
    pub fn wake(&mut self, body: BodyHandle) -> bool {
        let Some(i) = self.resolve(body) else {
            return false;
        };
        self.inner.wake_body(i);
        true
    }

    /// Whether the body sleeps (`None` if stale).
    #[must_use]
    pub fn is_sleeping(&self, body: BodyHandle) -> Option<bool> {
        self.resolve(body).map(|i| self.inner.is_sleeping(i))
    }

    /// Add a joint between two live bodies (`None` if either is stale, or
    /// both handles name the same body). The joint description is read in
    /// world space against the bodies' current poses (see
    /// [`crate::joint::JointKind`]). An inactive joint (`active == false`)
    /// gets a handle but constrains nothing.
    // By value per the module's API (a joint description is handed over).
    #[allow(clippy::needless_pass_by_value)]
    pub fn add_joint(&mut self, joint: crate::joint::Joint) -> Option<JointHandle> {
        let a = self.resolve(joint.body_a)?;
        let b = self.resolve(joint.body_b)?;
        if a == b {
            return None;
        }
        let inner = if joint.active {
            match joint_to_inner(
                &joint.kind,
                &self.inner.bodies[a],
                &self.inner.bodies[b],
                a,
                b,
            ) {
                InnerJointDesc::Joint(j) => {
                    let k = self.inner.add_joint(*j);
                    InnerJoint::Joint(k)
                }
                InnerJointDesc::Distance(c) => {
                    self.inner.add_distance_constraint(c);
                    InnerJoint::Distance(self.inner.distance_constraints.len() - 1)
                }
            }
        } else {
            InnerJoint::Inactive
        };
        self.wake_inner(a);
        self.wake_inner(b);

        let live = LiveJoint {
            inner,
            body_a: joint.body_a.index,
            body_b: joint.body_b.index,
        };
        let index = if let Some(index) = self.free_joints.pop() {
            self.joints[index as usize].live = Some(live);
            index
        } else {
            self.joints.push(JointSlot {
                generation: 0,
                live: Some(live),
            });
            u32::try_from(self.joints.len() - 1).expect("more than u32::MAX joints")
        };
        match inner {
            InnerJoint::Joint(_) => self.joint_to_slot.push(index),
            InnerJoint::Distance(_) => self.distance_to_slot.push(index),
            InnerJoint::Inactive => {}
        }
        Some(JointHandle {
            index,
            generation: self.joints[index as usize].generation,
        })
    }

    /// Remove a joint. `false` if stale.
    pub fn remove_joint(&mut self, joint: JointHandle) -> bool {
        if !self.contains_joint(joint) {
            return false;
        }
        let j = joint.index as usize;
        let live = self.joints[j].live.expect("checked by contains_joint");
        match live.inner {
            InnerJoint::Joint(k) => {
                self.inner.remove_joint(k);
                self.joint_to_slot.swap_remove(k);
            }
            InnerJoint::Distance(k) => {
                self.inner.distance_constraints.swap_remove(k);
                // The pub field bypasses the batch dirty flag; contacts are
                // re-detected every substep, so clearing them only marks the
                // constraint batches for rebuild.
                self.inner.clear_contacts();
                self.distance_to_slot.swap_remove(k);
            }
            InnerJoint::Inactive => {}
        }
        for b in [live.body_a, live.body_b] {
            if let Some(i) = self.bodies[b as usize].inner {
                self.wake_inner(i);
            }
        }
        self.kill_joint_slot(j);
        self.reindex_joints();
        true
    }

    /// Advance by `frame_dt` seconds of wall time; returns the number of
    /// fixed steps run (see the module contract).
    pub fn update(&mut self, frame_dt: f32) -> u32 {
        if !(frame_dt.is_finite() && frame_dt >= 0.0) {
            return 0;
        }
        self.accumulator += f64::from(frame_dt);
        let fixed = f64::from(self.config.fixed_dt);
        let mut steps = 0;
        loop {
            if self.accumulator < fixed {
                break;
            }
            if steps == self.config.max_steps_per_update {
                // Cap reached: drop the time left over (no catch-up spiral).
                self.accumulator = 0.0;
                break;
            }
            self.step_fixed();
            self.accumulator -= fixed;
            steps += 1;
        }
        steps
    }

    /// Run exactly one fixed step.
    pub fn step_fixed(&mut self) {
        let dt = self.fixed_dt_fix;
        for slot in &mut self.bodies {
            let Some(i) = slot.inner else { continue };
            if slot.force != Vec3::ZERO || slot.torque != Vec3::ZERO {
                let body = &mut self.inner.bodies[i];
                body.add_force(to_fix(slot.force), dt);
                body.add_torque(to_fix(slot.torque), dt);
                slot.force = Vec3::ZERO;
                slot.torque = Vec3::ZERO;
            }
        }
        self.inner.step(dt);
        self.collect_contacts();
    }

    /// Rebuild [`Self::contacts`] from the inner contact events.
    ///
    /// The inner sphere narrow phase reports the normal from the event's
    /// `body_b` to `body_a` (measured, see the `contact_normal_*` tests), so
    /// it is negated to point from `body_a` to `body_b`.
    fn collect_contacts(&mut self) {
        self.contacts.clear();
        for e in self.inner.contact_events() {
            if e.event_type == alice_physics::ContactEventType::End {
                continue;
            }
            let (Some(&sa), Some(&sb)) = (
                self.inner_to_body.get(e.body_a),
                self.inner_to_body.get(e.body_b),
            ) else {
                continue;
            };
            let handle = |s: u32| BodyHandle {
                index: s,
                generation: self.bodies[s as usize].generation,
            };
            self.contacts.push(Contact3D {
                body_a: handle(sa),
                body_b: handle(sb),
                point: from_fix(e.point),
                normal: -from_fix(e.normal),
                penetration: e.depth.to_f32().max(0.0),
            });
        }
    }

    /// Contacts of the last fixed step.
    #[must_use]
    pub fn contacts(&self) -> &[Contact3D] {
        &self.contacts
    }

    /// Sum of `½ m v² + ½ ωᵀ I ω` over dynamic bodies (J).
    #[must_use]
    pub fn total_kinetic_energy(&self) -> f32 {
        let mut energy = 0.0_f64;
        for b in &self.inner.bodies {
            if !b.is_dynamic() || b.inv_mass.is_zero() {
                continue;
            }
            let v = b.velocity;
            let (vx, vy, vz) = (v.x.to_f64(), v.y.to_f64(), v.z.to_f64());
            let v2 = vz.mul_add(vz, vy.mul_add(vy, vx * vx));
            energy += 0.5 * v2 / b.inv_mass.to_f64();
            let w = b.rotation.conjugate().rotate_vec(b.angular_velocity);
            for (wi, inv_i) in [
                (w.x, b.inv_inertia.x),
                (w.y, b.inv_inertia.y),
                (w.z, b.inv_inertia.z),
            ] {
                if !inv_i.is_zero() {
                    energy += 0.5 * wi.to_f64().powi(2) / inv_i.to_f64();
                }
            }
        }
        energy as f32
    }

    /// The inner `alice_physics` world (read access for features this
    /// adapter does not wrap). Index-based APIs of the inner world are not
    /// stable across `remove_body`; use [`PhysicsWorld::inner_index`].
    #[must_use]
    pub const fn inner(&self) -> &alice_physics::PhysicsWorld {
        &self.inner
    }

    /// Current inner index of a live body.
    #[must_use]
    pub fn inner_index(&self, body: BodyHandle) -> Option<usize> {
        self.resolve(body)
    }

    /// Live bodies with a collision sphere, as `(inner body, radius)`.
    fn colliding_bodies(&self) -> impl Iterator<Item = (&RigidBody, Fix128)> + '_ {
        self.bodies.iter().filter_map(|s| {
            let i = s.inner?;
            (s.radius > 0.0).then(|| (&self.inner.bodies[i], fx(s.radius)))
        })
    }
}

/// Scene queries for the engine (`EngineContext::set_collision_provider`):
/// every body with a collision sphere takes part, with its own radius.
impl crate::bridge::CollisionProvider for PhysicsWorld {
    fn sphere_cast(
        &self,
        origin: crate::math::Vec3,
        radius: f32,
        direction: crate::math::Vec3,
        max_distance: f32,
    ) -> Option<crate::bridge::CollisionHit> {
        let origin = to_fix(origin.0);
        let direction = to_fix(direction.0);
        let mut best = fx(max_distance);
        let mut hit = None;
        for (body, body_radius) in self.colliding_bodies() {
            if let Some(h) = alice_physics::query::sphere_cast(
                origin,
                fx(radius),
                direction,
                best,
                std::slice::from_ref(body),
                body_radius,
            ) {
                best = h.t;
                hit = Some(h);
            }
        }
        hit.map(|h| crate::bridge::CollisionHit {
            point: crate::math::Vec3(from_fix(h.point)),
            normal: crate::math::Vec3(from_fix(h.normal)),
            distance: h.t.to_f32(),
        })
    }

    fn aabb_overlap(&self, min: crate::math::Vec3, max: crate::math::Vec3) -> bool {
        let aabb = alice_physics::AABB::new(to_fix(min.0), to_fix(max.0));
        self.colliding_bodies().any(|(body, body_radius)| {
            !alice_physics::query::overlap_aabb_expanded(
                &aabb,
                std::slice::from_ref(body),
                body_radius,
            )
            .is_empty()
        })
    }
}

/// Sphere-trace a sphere of `radius` moving by `velocity · dt` from `start`
/// against the SDF `sdf_eval` (delegates to
/// `alice_physics::sdf_ccd::sphere_trace_sdf_field`, at most `max_steps`
/// iterations, contact tolerance 1 mm).
///
/// `time_of_impact` is at most the true time of impact and short of it by
/// about `0.001 / |velocity · dt|`. `None` when the sphere does not reach
/// the surface within `dt`, when `velocity · dt` is zero, or when the
/// iteration budget runs out.
pub fn sdf_ccd(
    sdf_eval: &dyn Fn(Vec3) -> f32,
    start: Vec3,
    velocity: Vec3,
    radius: f32,
    dt: f32,
    max_steps: u32,
) -> Option<CcdHit> {
    let displacement = velocity * dt;
    // The normal (used by the trace only for its contact point, which
    // `CcdHit` does not report) comes from alice_physics' central differences.
    let field = alice_physics::sdf_collider::DistanceSdfQuery::new(|x: f32, y: f32, z: f32| {
        sdf_eval(Vec3::new(x, y, z))
    });
    let config = alice_physics::SdfCcdConfig {
        max_iterations: max_steps as usize,
        ..alice_physics::SdfCcdConfig::default()
    };
    let toi = alice_physics::sdf_ccd::sphere_trace_sdf_field(
        to_fix(start),
        to_fix(displacement),
        fx(radius),
        &field,
        &alice_physics::sdf_collider::SdfFrame::IDENTITY,
        &config,
    )?;
    let time_of_impact = toi.t.to_f32();
    let position = start + displacement * time_of_impact;
    Some(CcdHit {
        position,
        time_of_impact,
        distance: sdf_eval(position),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::CollisionProvider;

    fn quiet(fixed_dt: f32) -> PhysicsWorld {
        PhysicsWorld::new(PhysicsConfig {
            gravity: Vec3::ZERO,
            fixed_dt,
            ..PhysicsConfig::default()
        })
    }

    fn ball(position: Vec3) -> BodyDesc {
        BodyDesc::dynamic(position, 1.0, 0.0)
    }

    fn unit_sphere(p: Vec3) -> f32 {
        p.length() - 1.0
    }

    #[test]
    fn ccd_hits_sphere() {
        let hit = sdf_ccd(
            &unit_sphere,
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, -10.0),
            0.1,
            1.0,
            64,
        );
        let h = hit.expect("the path crosses the sphere");
        assert!(h.time_of_impact > 0.0 && h.time_of_impact < 1.0);
    }

    #[test]
    fn ccd_misses() {
        let hit = sdf_ccd(
            &unit_sphere,
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
        assert!(sdf_ccd(
            &unit_sphere,
            Vec3::new(5.0, 0.0, 0.0),
            Vec3::ZERO,
            0.1,
            1.0,
            32
        )
        .is_none());
    }

    #[test]
    fn ccd_starting_inside_the_surface_hits_at_time_zero() {
        // Centre 0.5 inside the unit sphere: already penetrating, t = 0 and the
        // signed distance at the start (-0.5) is reported.
        let h = sdf_ccd(
            &unit_sphere,
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
            0.1,
            1.0,
            64,
        )
        .unwrap();
        assert!(
            h.time_of_impact.abs() < f32::EPSILON,
            "t {}",
            h.time_of_impact
        );
        assert_eq!(h.position, Vec3::new(0.5, 0.0, 0.0));
        assert!((h.distance + 0.5).abs() < 1e-6, "d {}", h.distance);
    }

    #[test]
    fn ccd_time_of_impact_matches_the_closed_form() {
        // Head-on: contact when |z| = R + r = 1.1, t* = (5 - 1.1) / 10 = 0.39.
        // The trace stops within the 1 mm tolerance before it: t ∈ [t* - 0.001/|d|, t*].
        let h = sdf_ccd(
            &unit_sphere,
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, -10.0),
            0.1,
            1.0,
            64,
        )
        .unwrap();
        assert!(
            h.time_of_impact <= 0.39 + 1e-6 && h.time_of_impact >= 0.39 - 2e-4,
            "t {}",
            h.time_of_impact
        );
        assert!(
            (h.position - Vec3::new(0.0, 0.0, 10.0_f32.mul_add(-h.time_of_impact, 5.0))).length()
                < 1e-5
        );
        assert!(
            h.distance >= 0.1 - 1e-6 && h.distance <= 0.1 + 1.1e-3,
            "d {}",
            h.distance
        );

        // Oblique, with dt: start (3, 0.6, 0) moving (-4, 0, 0) for dt 1.5 (d = (-6, 0, 0)).
        // Contact when x² + 0.36 = (1 + 0.2)², x = √1.08, t* = (3 - √1.08) / 6.
        let t_star = (3.0 - 1.08_f32.sqrt()) / 6.0;
        let h = sdf_ccd(
            &unit_sphere,
            Vec3::new(3.0, 0.6, 0.0),
            Vec3::new(-4.0, 0.0, 0.0),
            0.2,
            1.5,
            64,
        )
        .unwrap();
        assert!(
            h.time_of_impact <= t_star + 1e-6 && h.time_of_impact >= t_star - 2e-4,
            "t {} vs {t_star}",
            h.time_of_impact
        );
        // The sphere stops short of the surface when it would graze past it: no hit at y = 1.25 > 1.2.
        assert!(sdf_ccd(
            &unit_sphere,
            Vec3::new(3.0, 1.25, 0.0),
            Vec3::new(-4.0, 0.0, 0.0),
            0.2,
            1.5,
            64
        )
        .is_none());
        // Too short a sweep to reach it: |d| = 1.5 < 3 - 1.2.
        assert!(sdf_ccd(
            &unit_sphere,
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            0.2,
            1.5,
            64
        )
        .is_none());
    }

    #[test]
    fn config_defaults_follow_the_contract() {
        let c = PhysicsConfig::default();
        assert_eq!(c.gravity, Vec3::new(0.0, -9.81, 0.0));
        assert!((c.fixed_dt - 1.0 / 60.0).abs() < 1e-9);
        assert_eq!(c.substeps, 4);
        assert_eq!(c.max_steps_per_update, 8);
        assert_eq!(c.broadphase, Broadphase::DynamicTree);
        let w = PhysicsWorld::default();
        assert_eq!(w.inner().config.damping, Fix128::ONE);
        assert_eq!(w.inner().config.substeps, 4);
        assert_eq!(
            w.inner().broadphase(),
            alice_physics::solver::Broadphase::DynamicTree
        );
        let bvh = PhysicsWorld::new(PhysicsConfig {
            broadphase: Broadphase::Bvh,
            ..PhysicsConfig::default()
        });
        assert_eq!(
            bvh.inner().broadphase(),
            alice_physics::solver::Broadphase::Bvh
        );
    }

    #[test]
    fn free_fall_matches_semi_implicit_euler_closed_form() {
        // K = 60 · S substeps of h = 1/(60 S) s: v = -g K h = -g, x = -g h² K(K+1)/2
        let mut w = PhysicsWorld::default();
        let b = w.add_body(ball(Vec3::ZERO));
        for _ in 0..60 {
            w.step_fixed();
        }
        let k = 60.0 * f64::from(w.config().substeps);
        let (h, g) = (1.0 / k, 9.81_f64);
        let x = -g * h * h * k * (k + 1.0) / 2.0;
        let p = w.position(b).unwrap();
        let v = w.velocity(b).unwrap();
        assert!((f64::from(p.y) - x).abs() < 1e-4, "y {} vs {x}", p.y);
        assert!((f64::from(v.y) + g).abs() < 1e-4, "vy {}", v.y);
        assert!(p.x.abs() < f32::EPSILON);
    }

    #[test]
    fn damping_is_the_fraction_lost_per_second() {
        // linear_damping 0.5: after 1 s the speed is halved, whatever fixed_dt is
        for fixed_dt in [1.0 / 60.0, 1.0 / 120.0] {
            let mut w = quiet(fixed_dt);
            let mut d = ball(Vec3::ZERO);
            d.velocity = Vec3::new(2.0, 0.0, 0.0);
            d.linear_damping = 0.5;
            d.angular_velocity = Vec3::new(0.0, 4.0, 0.0);
            d.angular_damping = 0.75;
            let b = w.add_body(d);
            let steps = (1.0 / fixed_dt).round() as u32;
            for _ in 0..steps {
                w.step_fixed();
            }
            let v = w.velocity(b).unwrap();
            let omega = w.angular_velocity(b).unwrap();
            assert!((v.x - 1.0).abs() < 1e-4, "dt {fixed_dt}: vx {}", v.x);
            // The inner world re-derives ω from the orientation change each
            // substep, which by itself loses ~0.14 % over this second
            // (measured 4 → 3.9945 rad/s undamped), hence the wider bound.
            assert!(
                (omega.y - 1.0).abs() < 2e-3,
                "dt {fixed_dt}: wy {}",
                omega.y
            );
        }
    }

    #[test]
    fn zero_damping_keeps_velocity() {
        let mut w = quiet(1.0 / 60.0);
        let mut d = ball(Vec3::ZERO);
        d.velocity = Vec3::new(3.0, 0.0, 0.0);
        let b = w.add_body(d);
        for _ in 0..60 {
            w.step_fixed();
        }
        assert!((w.velocity(b).unwrap().x - 3.0).abs() < 1e-6);
        assert!((w.position(b).unwrap().x - 3.0).abs() < 1e-4);
    }

    #[test]
    fn update_runs_whole_fixed_steps_and_carries_the_remainder() {
        let mut w = quiet(0.25);
        assert_eq!(w.update(0.3), 1); // 0.05 left
        assert_eq!(w.update(0.3), 1); // 0.10 left
        assert_eq!(w.update(0.4), 2); // 0.00 left
        assert_eq!(w.update(0.2), 0);
        assert_eq!(w.update(0.05), 1);
        assert_eq!(w.update(10.0), 8, "capped by max_steps_per_update");
        assert_eq!(w.update(0.2), 0, "the capped remainder is dropped");
        assert_eq!(w.update(0.05), 1);
        assert_eq!(w.update(-1.0), 0);
        assert_eq!(w.update(f32::NAN), 0);
        assert_eq!(w.update(f32::INFINITY), 0);
        assert_eq!(
            w.update(0.25),
            1,
            "ignored input left the accumulator as it was"
        );
    }

    #[test]
    fn results_do_not_depend_on_the_frame_rate() {
        // Binary-exact times so that the frame sums are exact in f32.
        let run = |frame_dt: f32, frames: u32| {
            let mut w = PhysicsWorld::new(PhysicsConfig {
                fixed_dt: 1.0 / 64.0,
                ..PhysicsConfig::default()
            });
            let b = w.add_body(ball(Vec3::new(0.0, 10.0, 0.0)));
            let mut steps = 0;
            for _ in 0..frames {
                steps += w.update(frame_dt);
            }
            (steps, w.position(b).unwrap())
        };
        let (s64, p64) = run(1.0 / 64.0, 64);
        let (s16, p16) = run(1.0 / 16.0, 16);
        assert_eq!(s64, 64);
        assert_eq!(s16, 64);
        assert_eq!(p64, p16);
    }

    #[test]
    fn stale_handles_never_alias_later_bodies() {
        let mut w = quiet(1.0 / 60.0);
        let a = w.add_body(ball(Vec3::new(1.0, 0.0, 0.0)));
        let b = w.add_body(ball(Vec3::new(2.0, 0.0, 0.0)));
        let c = w.add_body(ball(Vec3::new(3.0, 0.0, 0.0)));
        assert!(w.remove_body(a));
        assert!(!w.remove_body(a));
        assert!(!w.contains(a));
        assert_eq!(w.position(a), None);
        assert!(!w.set_velocity(a, Vec3::X));
        // the inner swap-remove moved `c` into `a`'s place: handles still follow
        assert_eq!(w.position(b), Some(Vec3::new(2.0, 0.0, 0.0)));
        assert_eq!(w.position(c), Some(Vec3::new(3.0, 0.0, 0.0)));
        assert_eq!(w.inner_index(c), Some(0));
        // the freed slot is reused with a new generation
        let d = w.add_body(ball(Vec3::new(4.0, 0.0, 0.0)));
        assert_ne!(a, d);
        assert_eq!(w.position(a), None);
        assert_eq!(w.position(d), Some(Vec3::new(4.0, 0.0, 0.0)));
        assert_eq!(w.body_count(), 3);
    }

    #[test]
    fn impulse_force_and_torque_follow_newton() {
        let mut w = quiet(0.5);
        let b = w.add_body(BodyDesc::dynamic(Vec3::ZERO, 2.0, 1.0));
        // Δv = J / m
        assert!(w.apply_impulse(b, Vec3::new(4.0, 0.0, 0.0)));
        assert!((w.velocity(b).unwrap().x - 2.0).abs() < 1e-6);
        // F over one step: Δv = F dt / m = 8 · 0.5 / 2 = 2, then cleared
        assert!(w.apply_force(b, Vec3::new(0.0, 8.0, 0.0)));
        w.step_fixed();
        assert!((w.velocity(b).unwrap().y - 2.0).abs() < 1e-5);
        w.step_fixed();
        assert!(
            (w.velocity(b).unwrap().y - 2.0).abs() < 1e-5,
            "force acts for one step only"
        );
        // τ over one step on a solid sphere: Δω = τ dt / (2/5 m r²) = 0.8 · 0.5 / 0.8 = 0.5
        assert!(w.apply_torque(b, Vec3::new(0.0, 0.0, 0.8)));
        w.step_fixed();
        // (ω re-derived from the orientation change: measured 0.49984)
        assert!((w.angular_velocity(b).unwrap().z - 0.5).abs() < 1e-3);
    }

    #[test]
    fn kinetic_energy_is_translational_plus_rotational() {
        let mut w = quiet(1.0 / 60.0);
        let mut d = BodyDesc::dynamic(Vec3::ZERO, 2.0, 0.5);
        d.velocity = Vec3::new(3.0, 0.0, 0.0);
        d.angular_velocity = Vec3::new(0.0, 2.0, 0.0);
        w.add_body(d);
        w.add_body(BodyDesc::fixed(Vec3::new(100.0, 0.0, 0.0), 1.0));
        // ½·2·9 + ½·(0.4·2·0.25)·4 = 9 + 0.4
        assert!((w.total_kinetic_energy() - 9.4).abs() < 1e-5);
    }

    #[test]
    fn kinematic_body_reaches_its_target_in_one_step() {
        let mut w = quiet(1.0 / 60.0);
        let mut d = BodyDesc::fixed(Vec3::ZERO, 0.5);
        d.kind = BodyKind::Kinematic;
        let k = w.add_body(d);
        let s = w.add_body(BodyDesc::fixed(Vec3::new(5.0, 0.0, 0.0), 0.5));
        assert!(!w.set_kinematic_target(s, Vec3::ONE, Quat::IDENTITY));
        assert!(w.set_kinematic_target(k, Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY));
        w.step_fixed();
        assert!((w.position(k).unwrap() - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-5);
        assert_eq!(w.position(s), Some(Vec3::new(5.0, 0.0, 0.0)));
    }

    /// Two overlapping spheres, `first` added before `second`.
    fn contact_of(first: Vec3, second: Vec3) -> (BodyHandle, BodyHandle, Contact3D) {
        let mut w = quiet(1.0 / 60.0);
        let a = w.add_body(BodyDesc::dynamic(first, 1.0, 0.5));
        let b = w.add_body(BodyDesc::dynamic(second, 1.0, 0.5));
        w.step_fixed();
        assert_eq!(w.contacts().len(), 1);
        (a, b, w.contacts()[0])
    }

    #[test]
    fn broadphase_choice_does_not_change_the_result() {
        let run = |broadphase: Broadphase| {
            let mut w = PhysicsWorld::new(PhysicsConfig {
                broadphase,
                ..PhysicsConfig::default()
            });
            w.add_body(BodyDesc::fixed(Vec3::new(0.0, -100.0, 0.0), 100.0));
            let bodies: Vec<_> = (0..27)
                .map(|i| {
                    let p = Vec3::new((i % 3) as f32, 1.0 + (i / 9) as f32, ((i / 3) % 3) as f32);
                    w.add_body(BodyDesc::dynamic(p, 1.0, 0.5))
                })
                .collect();
            for _ in 0..120 {
                w.step_fixed();
            }
            bodies
                .iter()
                .map(|&b| w.position(b).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(run(Broadphase::Bvh), run(Broadphase::DynamicTree));
    }

    #[test]
    fn contact_normal_points_from_body_a_to_body_b() {
        // lower body added first
        let (a, b, c) = contact_of(Vec3::ZERO, Vec3::new(0.0, 0.9, 0.0));
        let n = if c.body_a == a { c.normal } else { -c.normal };
        assert_eq!((c.body_a, c.body_b), (a, b));
        assert!((n - Vec3::Y).length() < 1e-5, "normal {:?}", c.normal);
        assert!(
            (c.penetration - 0.1).abs() < 1e-5,
            "depth {}",
            c.penetration
        );
        // the point lies on body_a's surface facing body_b
        assert!(
            (c.point - Vec3::new(0.0, 0.5, 0.0)).length() < 1e-5,
            "point {:?}",
            c.point
        );
    }

    #[test]
    fn contact_normal_follows_the_pair_when_the_upper_body_comes_first() {
        let (a, b, c) = contact_of(Vec3::new(0.0, 0.9, 0.0), Vec3::ZERO);
        assert_eq!((c.body_a, c.body_b), (a, b));
        assert!(
            (c.normal + Vec3::Y).length() < 1e-5,
            "normal {:?}",
            c.normal
        );
        assert!(
            (c.point - Vec3::new(0.0, 0.4, 0.0)).length() < 1e-5,
            "point {:?}",
            c.point
        );
    }

    #[test]
    fn contact_normal_along_x_in_both_orders() {
        let (_, _, c) = contact_of(Vec3::ZERO, Vec3::new(0.8, 0.0, 0.0));
        assert!((c.normal - Vec3::X).length() < 1e-5);
        let (_, _, c) = contact_of(Vec3::new(0.8, 0.0, 0.0), Vec3::ZERO);
        assert!((c.normal + Vec3::X).length() < 1e-5);
    }

    #[test]
    fn a_body_with_radius_rests_on_the_ground_and_one_without_falls_through() {
        let mut w = PhysicsWorld::default();
        w.add_body(BodyDesc::fixed(Vec3::new(0.0, -100.0, 0.0), 100.0));
        let solid = w.add_body(BodyDesc::dynamic(Vec3::new(0.0, 2.0, 0.0), 1.0, 0.5));
        let ghost = w.add_body(BodyDesc::dynamic(Vec3::new(3.0, 2.0, 0.0), 1.0, 0.0));
        for _ in 0..240 {
            w.step_fixed();
        }
        let y = w.position(solid).unwrap().y;
        assert!((y - 0.5).abs() < 0.02, "resting height {y}");
        assert!(w.position(ghost).unwrap().y < -10.0);
    }

    #[test]
    fn head_on_spheres_do_not_pass_through_each_other() {
        let mut w = quiet(1.0 / 60.0);
        let mut da = BodyDesc::dynamic(Vec3::new(-1.0, 0.0, 0.0), 1.0, 0.5);
        da.velocity = Vec3::new(2.0, 0.0, 0.0);
        let mut db = BodyDesc::dynamic(Vec3::new(1.0, 0.0, 0.0), 1.0, 0.5);
        db.velocity = Vec3::new(-2.0, 0.0, 0.0);
        let a = w.add_body(da);
        let b = w.add_body(db);
        for _ in 0..120 {
            w.step_fixed();
            assert!(w.position(a).unwrap().x < w.position(b).unwrap().x);
        }
        // they separate again (restitution 0.3)
        assert!(w.velocity(a).unwrap().x < 0.0);
        assert!(w.velocity(b).unwrap().x > 0.0);
    }

    #[test]
    fn a_resting_body_falls_asleep_and_wakes() {
        let mut w = quiet(1.0 / 60.0);
        let b = w.add_body(BodyDesc::dynamic(Vec3::ZERO, 1.0, 0.5));
        for _ in 0..300 {
            w.step_fixed();
        }
        assert_eq!(w.is_sleeping(b), Some(true));
        assert!(w.wake(b));
        assert_eq!(w.is_sleeping(b), Some(false));
        assert!(w.apply_impulse(b, Vec3::X));
        w.step_fixed();
        assert!(w.position(b).unwrap().x > 0.0);
    }

    #[test]
    fn joints_follow_handles_across_body_removal() {
        use crate::joint::Joint;
        let mut w = quiet(1.0 / 60.0);
        let a = w.add_body(ball(Vec3::ZERO));
        let b = w.add_body(ball(Vec3::new(2.0, 0.0, 0.0)));
        let c = w.add_body(ball(Vec3::new(4.0, 0.0, 0.0)));
        let ab = w.add_joint(Joint::distance(a, b, 2.0)).unwrap();
        let bc = w
            .add_joint(Joint::ball(
                b,
                c,
                crate::math::Vec3::ZERO,
                crate::math::Vec3::new(-2.0, 0.0, 0.0),
            ))
            .unwrap();
        let ac = w.add_joint(Joint::spring(a, c, 4.0, 10.0, 0.0)).unwrap();
        assert_eq!(w.inner().joints.len(), 2);
        assert_eq!(w.inner().distance_constraints.len(), 1);
        assert!(w.remove_body(a));
        assert!(!w.contains_joint(ab));
        assert!(!w.contains_joint(ac));
        assert!(w.contains_joint(bc));
        assert_eq!(w.inner().joints.len(), 1);
        assert!(w.inner().distance_constraints.is_empty());
        assert!(w.remove_joint(bc));
        assert!(!w.remove_joint(bc));
        assert!(w.inner().joints.is_empty());
        assert_eq!(w.joint_count(), 0);
        assert!(
            w.add_joint(Joint::distance(a, b, 1.0)).is_none(),
            "stale body"
        );
        assert!(
            w.add_joint(Joint::distance(b, b, 1.0)).is_none(),
            "same body"
        );
    }

    #[test]
    fn removing_a_joint_frees_the_bodies() {
        use crate::joint::Joint;
        let mut w = quiet(1.0 / 60.0);
        let a = w.add_body(BodyDesc::fixed(Vec3::ZERO, 0.0));
        let b = w.add_body(ball(Vec3::new(2.0, 0.0, 0.0)));
        let d = w.add_joint(Joint::distance(a, b, 2.0)).unwrap();
        let j = w
            .add_joint(Joint::ball(
                a,
                b,
                crate::math::Vec3::ZERO,
                crate::math::Vec3::new(-2.0, 0.0, 0.0),
            ))
            .unwrap();
        assert!(w.remove_joint(d));
        assert!(w.remove_joint(j));
        w.set_velocity(b, Vec3::new(1.0, 0.0, 0.0));
        for _ in 0..60 {
            w.step_fixed();
        }
        assert!((w.position(b).unwrap().x - 3.0).abs() < 1e-3);
    }

    #[test]
    fn collision_provider_sphere_cast_and_aabb() {
        let mut w = quiet(1.0 / 60.0);
        w.add_body(BodyDesc::fixed(Vec3::new(5.0, 0.0, 0.0), 1.0));
        w.add_body(BodyDesc::fixed(Vec3::new(2.0, 0.0, 0.0), 0.0)); // no collision
        let v = crate::math::Vec3::new;
        // distance = 5 - (1 + 0.5)
        let hit = w
            .sphere_cast(v(0.0, 0.0, 0.0), 0.5, v(1.0, 0.0, 0.0), 100.0)
            .unwrap();
        assert!((hit.distance - 3.5).abs() < 1e-4, "{}", hit.distance);
        assert!((hit.normal.0 + Vec3::X).length() < 1e-4);
        assert!(w
            .sphere_cast(v(0.0, 0.0, 0.0), 0.5, v(1.0, 0.0, 0.0), 3.0)
            .is_none());
        assert!(w
            .sphere_cast(v(0.0, 0.0, 0.0), 0.5, v(0.0, 1.0, 0.0), 100.0)
            .is_none());
        assert!(w.aabb_overlap(v(3.5, -1.0, -1.0), v(4.5, 1.0, 1.0)));
        assert!(!w.aabb_overlap(v(1.5, -1.0, -1.0), v(2.5, 1.0, 1.0)));
    }
}
