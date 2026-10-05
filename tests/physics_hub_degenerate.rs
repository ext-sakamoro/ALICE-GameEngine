//! Degenerate-input tests for the `physics3d` adapter, driven through the
//! public entry points only. Each test states the expected outcome taken
//! from the module contract and asserts that value (not just "no panic").
//!
//! The degenerate-input rules (non-positive / non-finite mass, self joint,
//! bad `frame_dt`) are those of the module contract's "Degenerate input"
//! entry.

#![cfg(feature = "physics")]

use alice_game_engine::joint::Joint;
use alice_game_engine::physics3d::{
    BodyDesc, BodyHandle, BodyKind, Broadphase, PhysicsConfig, PhysicsWorld,
};
use glam::{Quat, Vec3};

const G: f64 = 9.81;
const EPS32: f64 = f32::EPSILON as f64;

fn config(gravity_y: f32) -> PhysicsConfig {
    PhysicsConfig {
        gravity: Vec3::new(0.0, gravity_y, 0.0),
        fixed_dt: 1.0 / 64.0,
        max_steps_per_update: 8,
        substeps: PhysicsConfig::default().substeps,
        broadphase: Broadphase::DynamicTree,
    }
}

/// Substep length `h = fixed_dt / substeps` of a world's config.
fn h_of(w: &PhysicsWorld) -> f64 {
    let c = w.config();
    f64::from(c.fixed_dt) / f64::from(c.substeps)
}

fn body(kind: BodyKind, position: Vec3, velocity: Vec3, mass: f32, radius: f32) -> BodyDesc {
    BodyDesc {
        kind,
        position,
        rotation: Quat::IDENTITY,
        velocity,
        angular_velocity: Vec3::ZERO,
        mass,
        radius,
        restitution: 0.0,
        friction: 0.0,
        linear_damping: 0.0,
        angular_damping: 0.0,
    }
}

fn bits(v: Vec3) -> [u32; 3] {
    v.to_array().map(f32::to_bits)
}

/// Contract: "`radius = 0` adds a body that does not collide".
///
/// Two radius-0 bodies flying through each other keep their free-flight
/// velocities exactly (gravity 0: `v` constant, `x = x0 + v t`), and a
/// radius-0 body dropped onto a static sphere falls through it along the
/// free-fall closed form `y = y0 − ½ g t²` (tolerance `½ g h t`, the
/// semi-implicit Euler error, see `physics_hub_analytic`). No contact is
/// ever reported.
#[test]
fn zero_radius_bodies_never_collide() {
    let mut w = PhysicsWorld::new(config(0.0));
    let a = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
        1.0,
        0.0,
    ));
    let b = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(-2.0, 0.0, 0.0),
        1.0,
        0.0,
    ));
    for _ in 0..64 {
        w.step_fixed();
        assert!(w.contacts().is_empty(), "radius 0 -> no contact");
    }
    assert_eq!(w.velocity(a), Some(Vec3::new(2.0, 0.0, 0.0)));
    assert_eq!(w.velocity(b), Some(Vec3::new(-2.0, 0.0, 0.0)));
    let (pa, pb) = (w.position(a).expect("live"), w.position(b).expect("live"));
    assert!(
        (pa.x - 1.0).abs() <= 1e-5 && (pb.x + 1.0).abs() <= 1e-5,
        "passed through: {pa:?} {pb:?}"
    );

    let mut w = PhysicsWorld::new(config(-9.81));
    w.add_body(body(
        BodyKind::Static,
        Vec3::new(0.0, -100.0, 0.0),
        Vec3::ZERO,
        0.0,
        100.0,
    ));
    let y0 = 1.0_f64;
    let d = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new(0.0, y0 as f32, 0.0),
        Vec3::ZERO,
        1.0,
        0.0,
    ));
    let n = 64_u32;
    for _ in 0..n {
        w.step_fixed();
        assert!(
            w.contacts().is_empty(),
            "radius 0 body against static ground -> no contact"
        );
    }
    let t = f64::from(n) / 64.0;
    let h = h_of(&w);
    let y = f64::from(w.position(d).expect("live").y);
    let y_exp = y0 - 0.5 * G * t * t;
    assert!(
        (y - y_exp).abs() <= 0.5 * G * h * t + 16.0 * EPS32 * 5.0,
        "falls through the ground: y {y}, free fall {y_exp}"
    );
}

/// An empty world advances time like any other: `update(1/64)` with
/// `fixed_dt = 1/64` runs 1 step, there are no bodies, no contacts and no
/// kinetic energy.
#[test]
fn empty_world_update() {
    let mut w = PhysicsWorld::new(config(-9.81));
    assert_eq!(w.update(1.0 / 64.0), 1);
    w.step_fixed();
    assert_eq!(w.body_count(), 0);
    assert!(w.contacts().is_empty());
    assert_eq!(w.total_kinetic_energy(), 0.0);
}

/// A negative or NaN frame time is not time that has passed: `update`
/// returns 0 steps and the body state is bit-identical to before.
/// (That it is also kept out of the accumulator is checked in
/// `bad_frame_time_does_not_poison_the_accumulator`.)
#[test]
fn negative_or_nan_frame_time_runs_no_step() {
    for bad in [-1.0_f32, -1.0 / 64.0, f32::NAN, f32::NEG_INFINITY] {
        let mut w = PhysicsWorld::new(config(-9.81));
        let b = w.add_body(body(
            BodyKind::Dynamic,
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            1.0,
            0.5,
        ));
        let (p, v) = (w.position(b).expect("live"), w.velocity(b).expect("live"));
        assert_eq!(w.update(bad), 0, "update({bad}) runs no step");
        assert_eq!(
            bits(w.position(b).expect("live")),
            bits(p),
            "update({bad}) leaves the position"
        );
        assert_eq!(
            bits(w.velocity(b).expect("live")),
            bits(v),
            "update({bad}) leaves the velocity"
        );
    }
}

/// Contract: "a removed handle ... every accessor returns `None` / `false`".
/// Checked on every accessor, and none of the calls touches the live body.
#[test]
fn stale_handle_is_rejected_by_every_accessor() {
    let mut w = PhysicsWorld::new(config(0.0));
    let live = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new(3.0, 0.0, 0.0),
        Vec3::ZERO,
        1.0,
        0.5,
    ));
    let dead = w.add_body(body(BodyKind::Kinematic, Vec3::ZERO, Vec3::ZERO, 1.0, 0.5));
    assert!(w.remove_body(dead));

    assert!(!w.contains(dead));
    assert_eq!(w.position(dead), None);
    assert_eq!(w.rotation(dead), None);
    assert_eq!(w.velocity(dead), None);
    assert_eq!(w.angular_velocity(dead), None);
    assert_eq!(w.is_sleeping(dead), None);
    assert_eq!(w.inner_index(dead), None);
    assert!(!w.set_position(dead, Vec3::ONE));
    assert!(!w.set_velocity(dead, Vec3::ONE));
    assert!(!w.set_kinematic_target(dead, Vec3::ONE, Quat::IDENTITY));
    assert!(!w.apply_impulse(dead, Vec3::ONE));
    assert!(!w.apply_force(dead, Vec3::ONE));
    assert!(!w.apply_torque(dead, Vec3::ONE));
    assert!(!w.wake(dead));
    assert!(!w.remove_body(dead));
    assert_eq!(
        w.add_joint(Joint::distance(live, dead, 1.0)),
        None,
        "joint to a stale body"
    );
    assert_eq!(
        w.add_joint(Joint::distance(dead, live, 1.0)),
        None,
        "joint from a stale body"
    );

    assert_eq!(w.body_count(), 1);
    w.step_fixed();
    assert_eq!(
        w.position(live),
        Some(Vec3::new(3.0, 0.0, 0.0)),
        "live body untouched"
    );
    assert_eq!(w.velocity(live), Some(Vec3::ZERO));
}

/// A removed joint handle is stale: the second `remove_joint` returns
/// `false`. Removing a body also removes its joints (contract of
/// `remove_body`), so that joint's handle is stale as well.
#[test]
fn stale_joint_handles() {
    let mut w = PhysicsWorld::new(config(0.0));
    let a = w.add_body(body(BodyKind::Dynamic, Vec3::ZERO, Vec3::ZERO, 1.0, 0.0));
    let b = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::ZERO,
        1.0,
        0.0,
    ));
    let c = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::ZERO,
        1.0,
        0.0,
    ));
    let j1 = w.add_joint(Joint::distance(a, b, 1.0)).expect("live pair");
    let j2 = w.add_joint(Joint::distance(b, c, 1.0)).expect("live pair");
    assert_ne!(j1, j2);
    assert!(w.remove_joint(j1));
    assert!(!w.remove_joint(j1), "second remove of the same joint");
    assert!(w.remove_body(c));
    assert!(!w.remove_joint(j2), "joint removed together with its body");
}

/// `set_kinematic_target` is "`false` if stale or not kinematic": on a
/// dynamic and on a static body it returns `false` and the body's pose is
/// unchanged.
#[test]
fn kinematic_target_rejected_on_non_kinematic_bodies() {
    let mut w = PhysicsWorld::new(config(0.0));
    let d = w.add_body(body(BodyKind::Dynamic, Vec3::ZERO, Vec3::ZERO, 1.0, 0.0));
    let s = w.add_body(body(
        BodyKind::Static,
        Vec3::new(5.0, 0.0, 0.0),
        Vec3::ZERO,
        0.0,
        0.0,
    ));
    assert!(!w.set_kinematic_target(d, Vec3::ONE, Quat::IDENTITY));
    assert!(!w.set_kinematic_target(s, Vec3::ONE, Quat::IDENTITY));
    w.step_fixed();
    assert_eq!(w.position(d), Some(Vec3::ZERO));
    assert_eq!(w.position(s), Some(Vec3::new(5.0, 0.0, 0.0)));
}

/// A stale handle compares unequal to every live handle (generation), so
/// it cannot be confused with a body added into the freed slot.
#[test]
fn stale_handle_differs_from_all_later_handles() {
    let mut w = PhysicsWorld::new(config(0.0));
    let first = w.add_body(body(BodyKind::Dynamic, Vec3::ZERO, Vec3::ZERO, 1.0, 0.0));
    assert!(w.remove_body(first));
    let later: Vec<BodyHandle> = (0..4)
        .map(|i| {
            w.add_body(body(
                BodyKind::Dynamic,
                Vec3::new(i as f32, 0.0, 0.0),
                Vec3::ZERO,
                1.0,
                0.0,
            ))
        })
        .collect();
    for h in &later {
        assert_ne!(*h, first);
        assert!(w.contains(*h));
    }
    assert_eq!(w.position(first), None);
}

/// Contract: a `Dynamic` body whose mass is not finite and positive is
/// added as `Static`. Under gravity −9.81 and with an initial velocity it
/// therefore never moves: its position after 64 steps is bit-identical to
/// the initial one. A dynamic ball dropped onto it from 0.1 m above is
/// stopped by it (a static body collides), i.e. stays above `y = 1.0`
/// (surfaces touch at centre distance 1.0; allowance `g h²` per the
/// resting-contact derivation in `physics_hub_analytic`).
#[test]
fn dynamic_with_bad_mass_is_added_as_static() {
    for m in [0.0_f32, -1.0, f32::NAN, f32::INFINITY] {
        let mut w = PhysicsWorld::new(config(-9.81));
        let p0 = Vec3::new(0.0, 0.0, 0.0);
        let s = w.add_body(body(
            BodyKind::Dynamic,
            p0,
            Vec3::new(1.0, 2.0, 3.0),
            m,
            0.5,
        ));
        let d = w.add_body(body(
            BodyKind::Dynamic,
            Vec3::new(0.0, 1.1, 0.0),
            Vec3::ZERO,
            1.0,
            0.5,
        ));
        for _ in 0..64 {
            w.step_fixed();
        }
        assert_eq!(
            bits(w.position(s).expect("live")),
            bits(p0),
            "mass {m}: body added as Static does not move"
        );
        let y = f64::from(w.position(d).expect("live").y);
        let h = h_of(&w);
        assert!(
            y >= 1.0 - G * h * h - 16.0 * EPS32,
            "mass {m}: the static body stops a falling ball, y = {y}"
        );
    }
}

/// Contract: a joint between a body and itself returns `None`, and adding it
/// leaves the body free (it still falls by the free-fall closed form,
/// tolerance `½ g h t`).
#[test]
fn self_joint_returns_none() {
    let mut w = PhysicsWorld::new(config(-9.81));
    let a = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new(0.0, 10.0, 0.0),
        Vec3::ZERO,
        1.0,
        0.0,
    ));
    assert_eq!(w.add_joint(Joint::distance(a, a, 1.0)), None);
    for _ in 0..64 {
        w.step_fixed();
    }
    let y = f64::from(w.position(a).expect("live").y);
    let (t, h) = (1.0, h_of(&w));
    assert!(
        (y - (10.0 - 0.5 * G * t * t)).abs() <= 0.5 * G * h * t + 16.0 * 10.0 * EPS32,
        "free fall unaffected: y = {y}"
    );
}

/// Contract: a negative, NaN or infinite `frame_dt` is not added to the
/// accumulator, so a later normal frame behaves as if it never came:
/// `update(1/64)` runs exactly 1 step, and the state equals one fixed step
/// of an untouched world (bit for bit). `update(+∞)` is included and must
/// also run 0.
#[test]
fn bad_frame_time_does_not_poison_the_accumulator() {
    for bad in [-1.0_f32, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut w = PhysicsWorld::new(config(-9.81));
        let b = w.add_body(body(
            BodyKind::Dynamic,
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            1.0,
            0.0,
        ));
        assert_eq!(w.update(bad), 0, "update({bad}) runs no step");
        assert_eq!(
            w.update(1.0 / 64.0),
            1,
            "after update({bad}), one frame = one step"
        );
        assert_eq!(
            w.update(0.5 / 64.0),
            0,
            "after update({bad}), no extra time is held"
        );

        let mut r = PhysicsWorld::new(config(-9.81));
        let rb = r.add_body(body(
            BodyKind::Dynamic,
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            1.0,
            0.0,
        ));
        r.step_fixed();
        assert_eq!(
            bits(w.position(b).expect("live")),
            bits(r.position(rb).expect("live"))
        );
        assert_eq!(
            bits(w.velocity(b).expect("live")),
            bits(r.velocity(rb).expect("live"))
        );
    }
}
