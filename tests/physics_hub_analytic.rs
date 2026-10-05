//! Analytic oracles for the `physics3d` adapter, driven through the public
//! entry points only (`PhysicsWorld::new` / `add_body` / `update` /
//! `step_fixed` / accessors).
//!
//! Every expected value comes from a closed form evaluated in `f64` here;
//! no function of the crate is called to build an expectation. Each test
//! documents its closed form, where it comes from, and how its tolerance is
//! derived (fixed-step discretisation, substeps, `f32` boundary rounding).
//!
//! Integrator model used for the tolerances: within one fixed step of
//! length `dt` the inner world runs `S = substeps` substeps of `h = dt / S`,
//! each `v += g h; x += v h` (semi-implicit Euler) followed by position
//! projection and `v = Δx / h`.

#![cfg(feature = "physics")]

use alice_game_engine::joint::Joint;
use alice_game_engine::physics3d::{
    BodyDesc, BodyHandle, BodyKind, Broadphase, PhysicsConfig, PhysicsWorld,
};
use glam::{Quat, Vec3};

const G: f64 = 9.81;
/// One `f32` ulp at 1.0.
const EPS32: f64 = f32::EPSILON as f64;

fn config(gravity_y: f32, fixed_dt: f32, substeps: u32, max_steps: u32) -> PhysicsConfig {
    PhysicsConfig {
        gravity: Vec3::new(0.0, gravity_y, 0.0),
        fixed_dt,
        substeps,
        max_steps_per_update: max_steps,
        broadphase: Broadphase::DynamicTree,
    }
}

/// Default substep count (scenes follow the default; every tolerance reads
/// `h` back from the world's config instead of assuming a number).
fn default_substeps() -> u32 {
    PhysicsConfig::default().substeps
}

/// Substep length `h = fixed_dt / substeps` of a world's config.
fn h_of(w: &PhysicsWorld) -> f64 {
    let c = w.config();
    f64::from(c.fixed_dt) / f64::from(c.substeps)
}

/// Body with every field explicit (no reliance on `BodyDesc` constructors'
/// defaults): no damping, no friction, no restitution unless set.
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

fn pos(w: &PhysicsWorld, b: BodyHandle) -> Vec3 {
    w.position(b).expect("live handle has a position")
}

fn vel(w: &PhysicsWorld, b: BodyHandle) -> Vec3 {
    w.velocity(b).expect("live handle has a velocity")
}

// ---------------------------------------------------------------------------
// 1. Free fall
// ---------------------------------------------------------------------------

/// Free fall from rest with the default config.
///
/// Closed form (Newton): `v(t) = -g t`, `y(t) = y0 - ½ g t²`, `g = 9.81`
/// (the module contract fixes the default gravity to `(0, -9.81, 0)`).
///
/// Tolerance:
/// - `v`: semi-implicit Euler gives `v_N = -g N h = -g t` exactly, so only
///   rounding remains: `|v| · 16 ε₃₂` (f32 ↔ fixed point of `g`, `dt`, and
///   the f32 read-out).
/// - `y`: semi-implicit Euler gives `y_N = y0 - g h² N(N+1)/2
///   = y0 - ½ g t² - ½ g h t`, i.e. a first-order error of exactly
///   `½ g h t` with `h = fixed_dt / substeps`; plus `|y| · 16 ε₃₂` rounding.
#[test]
fn free_fall_default_config_matches_closed_form() {
    let mut w = PhysicsWorld::new(PhysicsConfig::default());
    let cfg = w.config();
    assert_eq!(
        cfg.gravity,
        Vec3::new(0.0, -9.81, 0.0),
        "default gravity is the contract value"
    );

    let y0 = 100.0_f64;
    let b = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new(0.0, y0 as f32, 0.0),
        Vec3::ZERO,
        1.0,
        0.0,
    ));

    let dt = f64::from(cfg.fixed_dt);
    let h = dt / f64::from(cfg.substeps);
    let n = 60_u32;
    for _ in 0..n {
        w.step_fixed();
    }
    let t = f64::from(n) * dt;

    let v_exp = -G * t;
    let y_exp = y0 - 0.5 * G * t * t;
    let v = f64::from(vel(&w, b).y);
    let y = f64::from(pos(&w, b).y);

    let tol_v = v_exp.abs() * 16.0 * EPS32;
    let tol_y = 0.5 * G * h * t + y0 * 16.0 * EPS32;
    assert!(
        (v - v_exp).abs() <= tol_v,
        "v = {v}, closed form {v_exp}, tol {tol_v}"
    );
    assert!(
        (y - y_exp).abs() <= tol_y,
        "y = {y}, closed form {y_exp}, tol {tol_y}"
    );
    let p = pos(&w, b);
    assert!(p.x == 0.0 && p.z == 0.0, "no lateral motion: {p:?}");
}

/// The free-fall error shrinks with the substep length: for substeps
/// 1, 4, 16 the position error stays within `½ g (dt/S) t` (same derivation
/// as above). A damping or gravity applied per substep instead of per step
/// would make the result depend on `S` beyond this bound.
#[test]
fn free_fall_error_is_bounded_by_substep_length() {
    let dt = 1.0_f32 / 64.0;
    for s in [1_u32, 4, 16] {
        let mut w = PhysicsWorld::new(config(-9.81, dt, s, 8));
        let y0 = 50.0_f64;
        let b = w.add_body(body(
            BodyKind::Dynamic,
            Vec3::new(0.0, y0 as f32, 0.0),
            Vec3::ZERO,
            1.0,
            0.0,
        ));
        let n = 64_u32;
        for _ in 0..n {
            w.step_fixed();
        }
        let t = f64::from(n) * f64::from(dt);
        let h = h_of(&w);
        let y_exp = y0 - 0.5 * G * t * t;
        let v_exp = -G * t;
        let y = f64::from(pos(&w, b).y);
        let v = f64::from(vel(&w, b).y);
        let tol_y = 0.5 * G * h * t + y0 * 16.0 * EPS32;
        assert!(
            (y - y_exp).abs() <= tol_y,
            "substeps {s}: y = {y}, closed form {y_exp}, tol {tol_y}"
        );
        assert!(
            (v - v_exp).abs() <= v_exp.abs() * 16.0 * EPS32,
            "substeps {s}: v = {v}, closed form {v_exp}"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Projectile range
// ---------------------------------------------------------------------------

/// Launch at `v0 = 10 m/s`, `θ = 30°` from `y = 0` (no ground: radius 0).
///
/// Closed form: range `R = v0² sin 2θ / g` (= 8.8279 m).
///
/// The landing x is read where `y` crosses 0 downwards, by linear
/// interpolation between the two fixed steps around the crossing.
///
/// Tolerance:
/// - discretisation: with semi-implicit Euler `y(t) = v_y t - ½ g t² -
///   ½ g h t`, so the landing time is `2 v_y / g - h` and the range is short
///   by `v_x h`;
/// - interpolation: `|y''| = g`, so linear interpolation over one step is off
///   by at most `g dt² / 8` in `y`, i.e. `g dt² / (8 |v_y|)` in time and
///   `v_x g dt² / (8 |v_y|)` in x;
/// - rounding: `R · 16 ε₃₂`.
#[test]
fn projectile_range_matches_closed_form() {
    let dt = 1.0_f32 / 60.0;
    let s = default_substeps();
    let mut w = PhysicsWorld::new(config(-9.81, dt, s, 8));
    let v0 = 10.0_f64;
    let theta = 30.0_f64.to_radians();
    let (vx, vy) = (v0 * theta.cos(), v0 * theta.sin());
    let b = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::ZERO,
        Vec3::new(vx as f32, vy as f32, 0.0),
        1.0,
        0.0,
    ));

    let mut prev = pos(&w, b);
    let mut landing_x = None;
    for _ in 0..600 {
        w.step_fixed();
        let p = pos(&w, b);
        if prev.y > 0.0 && p.y <= 0.0 {
            let (y0, y1) = (f64::from(prev.y), f64::from(p.y));
            let f = y0 / (y0 - y1);
            landing_x = Some(f64::from(prev.x) + f * (f64::from(p.x) - f64::from(prev.x)));
            break;
        }
        prev = p;
    }
    let x = landing_x.expect("the projectile comes back down to y = 0");

    let r = v0 * v0 * (2.0 * theta).sin() / G;
    let dt = f64::from(dt);
    let h = h_of(&w);
    let tol = vx * h + vx * G * dt * dt / (8.0 * vy) + r * 16.0 * EPS32;
    assert!(
        (x - r).abs() <= tol,
        "range {x}, closed form {r}, tol {tol}"
    );
}

// ---------------------------------------------------------------------------
// 3. Impulse
// ---------------------------------------------------------------------------

/// `apply_impulse` is an instant velocity change `Δv = J / m`
/// (impulse–momentum theorem); two impulses add. With gravity 0 and no
/// damping the velocity then survives a step unchanged, and the position
/// advances by exactly `v · dt` (`S` substeps of `v h`).
///
/// Tolerance: rounding only, `|v| · 16 ε₃₂` (`J / m` is evaluated across the
/// f32 ↔ fixed point boundary).
#[test]
fn impulse_changes_velocity_by_j_over_m_and_accumulates() {
    let dt = 1.0_f32 / 64.0;
    let mut w = PhysicsWorld::new(config(0.0, dt, default_substeps(), 8));
    let m = 2.0_f32;
    let b = w.add_body(body(BodyKind::Dynamic, Vec3::ZERO, Vec3::ZERO, m, 0.0));

    assert!(w.apply_impulse(b, Vec3::new(3.0, 0.0, 0.0)));
    let v1 = vel(&w, b);
    let tol = 1.5 * 16.0 * EPS32;
    assert!(
        (f64::from(v1.x) - 1.5).abs() <= tol && v1.y == 0.0 && v1.z == 0.0,
        "after J=(3,0,0) on m=2: {v1:?}"
    );

    assert!(w.apply_impulse(b, Vec3::new(0.0, 4.0, 0.0)));
    let v2 = vel(&w, b);
    assert!(
        (f64::from(v2.x) - 1.5).abs() <= tol
            && (f64::from(v2.y) - 2.0).abs() <= 2.0 * 16.0 * EPS32
            && v2.z == 0.0,
        "impulses add: {v2:?}"
    );

    w.step_fixed();
    let v3 = vel(&w, b);
    let p3 = pos(&w, b);
    assert!(
        (f64::from(v3.x) - 1.5).abs() <= tol && (f64::from(v3.y) - 2.0).abs() <= 2.0 * 16.0 * EPS32,
        "velocity kept over a step: {v3:?}"
    );
    let dt = f64::from(dt);
    assert!(
        (f64::from(p3.x) - 1.5 * dt).abs() <= 1e-6 && (f64::from(p3.y) - 2.0 * dt).abs() <= 1e-6,
        "moved by v·dt: {p3:?}"
    );
}

// ---------------------------------------------------------------------------
// 4. Force
// ---------------------------------------------------------------------------

/// A force acts over the next fixed step only, so it is re-applied every
/// step. After `k` steps of constant `F` on mass `m` (gravity 0):
/// `v = F/m · k dt` (Newton's 2nd law, exact for any split of the step's
/// impulse `F dt` across substeps), and `x = ½ a t²` with `a = F/m`,
/// `t = k dt`.
///
/// Then `k2` steps without a force leave `v` unchanged (the force cleared).
///
/// Tolerance:
/// - `v`: rounding, `|v| · 16 ε₃₂ · k` (one conversion per step);
/// - `x`: the contract fixes only "over the next fixed step", not how the
///   impulse is spread inside the step; the worst case (all at the start of
///   each step) gives `x = a dt² k(k+1)/2 = ½ a t² + ½ a dt t`, so the bound
///   is `½ a dt t` plus rounding.
#[test]
fn constant_force_reapplied_each_step_gives_f_over_m_t() {
    let dt = 1.0_f32 / 60.0;
    let mut w = PhysicsWorld::new(config(0.0, dt, default_substeps(), 8));
    let m = 2.0_f64;
    let f = 4.0_f64;
    let b = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::ZERO,
        Vec3::ZERO,
        m as f32,
        0.0,
    ));

    let k = 30_u32;
    for _ in 0..k {
        assert!(w.apply_force(b, Vec3::new(f as f32, 0.0, 0.0)));
        w.step_fixed();
    }
    let dt = f64::from(dt);
    let t = f64::from(k) * dt;
    let a = f / m;
    let v_exp = a * t;
    let x_exp = 0.5 * a * t * t;
    let v = f64::from(vel(&w, b).x);
    let x = f64::from(pos(&w, b).x);
    let tol_v = v_exp * 16.0 * EPS32 * f64::from(k);
    assert!(
        (v - v_exp).abs() <= tol_v,
        "v = {v}, closed form F/m·k·dt = {v_exp}, tol {tol_v}"
    );
    let tol_x = 0.5 * a * dt * t + 1e-6;
    assert!(
        (x - x_exp).abs() <= tol_x,
        "x = {x}, closed form ½at² = {x_exp}, tol {tol_x}"
    );

    for _ in 0..10 {
        w.step_fixed();
    }
    let v_after = f64::from(vel(&w, b).x);
    assert!(
        (v_after - v_exp).abs() <= tol_v,
        "force does not persist: v = {v_after}, expected {v_exp}"
    );
}

// ---------------------------------------------------------------------------
// 5. Damping
// ---------------------------------------------------------------------------

/// `linear_damping = d` is the fraction of velocity lost per second
/// (module contract), so with gravity 0: `v(1 s) = v0 (1 − d)`,
/// `v(2 s) = v0 (1 − d)²`. Same for `angular_damping` and `ω`.
///
/// `d = 0.5`, `v0 = 10 m/s`, `ω0 = 0.5 rad/s`, `fixed_dt = 1/64` (so 1 s is
/// exactly 64 steps). A linear per-step conversion `1 − d dt` would give
/// `(1 − 0.5/64)^64 = 0.6055` instead of 0.5.
///
/// Tolerance:
/// - linear: the per-step retention factor is rounded to f32 at most once
///   per step, so after `N` steps the relative error is `N · ε₃₂`;
/// - angular: in addition the inner world re-derives `ω` from the rotation
///   change each substep as `2 sin(ω h / 2) / h`, which loses `(ω h)² / 24`
///   relative per substep, i.e. `N S (ω0 h)² / 24` over `N S` substeps.
#[test]
fn damping_is_fraction_lost_per_second() {
    let dt = 1.0_f32 / 64.0;
    let s = default_substeps();
    let mut w = PhysicsWorld::new(config(0.0, dt, s, 8));
    let d = 0.5_f64;
    let v0 = 10.0_f64;
    let w0 = 0.5_f64;
    let mut desc = body(
        BodyKind::Dynamic,
        Vec3::ZERO,
        Vec3::new(v0 as f32, 0.0, 0.0),
        1.0,
        0.0,
    );
    desc.linear_damping = d as f32;
    desc.angular_damping = d as f32;
    desc.angular_velocity = Vec3::new(0.0, 0.0, w0 as f32);
    let b = w.add_body(desc);

    let h = h_of(&w);
    for (seconds, n_total) in [(1_i32, 64_u32), (2, 128)] {
        while_steps(&mut w, 64);
        let n = f64::from(n_total);
        let v_exp = v0 * (1.0 - d).powi(seconds);
        let w_exp = w0 * (1.0 - d).powi(seconds);
        let v = f64::from(vel(&w, b).x);
        let om = f64::from(w.angular_velocity(b).expect("live").z);
        let tol_v = v_exp * n * EPS32;
        let tol_w =
            w_exp * (n * EPS32 + n * f64::from(w.config().substeps) * (w0 * h).powi(2) / 24.0);
        assert!(
            (v - v_exp).abs() <= tol_v,
            "{seconds} s: v = {v}, closed form v0(1-d)^t = {v_exp}, tol {tol_v}"
        );
        assert!(
            (om - w_exp).abs() <= tol_w,
            "{seconds} s: ω = {om}, closed form ω0(1-d)^t = {w_exp}, tol {tol_w}"
        );
    }
}

fn while_steps(w: &mut PhysicsWorld, n: u32) {
    for _ in 0..n {
        w.step_fixed();
    }
}

// ---------------------------------------------------------------------------
// 6. Head-on collisions
// ---------------------------------------------------------------------------

/// Run a 1-D head-on collision of two spheres (radius 0.5, gravity 0,
/// friction 0, both bodies with restitution `e`) and return the final
/// velocities along x. The pair starts 3 m apart (surface gap) and closes at
/// `v1 − v2`; 1.5 s is long enough for contact and separation.
fn head_on(m1: f32, v1: f32, m2: f32, v2: f32, e: f32) -> (f64, f64) {
    head_on_pair(m1, v1, m2, v2, e, e)
}

/// As [`head_on`], with a separate restitution per body.
fn head_on_pair(m1: f32, v1: f32, m2: f32, v2: f32, ea: f32, eb: f32) -> (f64, f64) {
    head_on_offset(m1, v1, m2, v2, ea, eb, 0.0)
}

/// As [`head_on_pair`], with the second body moved `offset` metres further
/// away (changes the substep phase at which contact begins).
fn head_on_offset(m1: f32, v1: f32, m2: f32, v2: f32, ea: f32, eb: f32, offset: f32) -> (f64, f64) {
    let mut w = PhysicsWorld::new(config(0.0, 1.0 / 60.0, default_substeps(), 8));
    let mut a = body(
        BodyKind::Dynamic,
        Vec3::new(-2.0, 0.0, 0.0),
        Vec3::new(v1, 0.0, 0.0),
        m1,
        0.5,
    );
    let mut b = body(
        BodyKind::Dynamic,
        Vec3::new(2.0 + offset, 0.0, 0.0),
        Vec3::new(v2, 0.0, 0.0),
        m2,
        0.5,
    );
    a.restitution = ea;
    b.restitution = eb;
    let ha = w.add_body(a);
    let hb = w.add_body(b);
    while_steps(&mut w, 90);
    let (va, vb) = (vel(&w, ha), vel(&w, hb));
    assert!(
        va.y.abs() < 1e-6 && va.z.abs() < 1e-6 && vb.y.abs() < 1e-6 && vb.z.abs() < 1e-6,
        "1-D collision stays on x"
    );
    (f64::from(va.x), f64::from(vb.x))
}

/// Velocity tolerance of the collision tests (see the derivation below).
fn tol_v(v1: f64, v2: f64) -> f64 {
    (v1.abs() + v2.abs()) * 16.0 * EPS32 * f64::from(default_substeps())
}

/// Newton's experimental law + momentum conservation for a 1-D collision:
/// `m1 v1 + m2 v2 = m1 v1' + m2 v2'` and `v2' − v1' = e (v1 − v2)`.
/// Upper bound: kinetic energy does not grow.
///
/// Scene: `m1 = 1, v1 = +4`, `m2 = 3, v2 = −2`, `e = 0.5` on both bodies
/// (the contract does not fix how two restitutions combine; equal values
/// make every usual rule — average, min, max, geometric mean — give `e`).
/// Closed form: `v1' = −2.75`, `v2' = +0.25`.
///
/// Tolerance:
/// - momentum: rounding only, `(m1|v1| + m2|v2|) · 16 ε₃₂` (projection and
///   impulses act on mass-weighted pairs, which conserve momentum exactly in
///   exact arithmetic);
/// - velocities / separation: with gravity 0 the pre-contact relative
///   velocity is exact and Newton's law applied to it is exact, so only the
///   f32 ↔ fixed point rounding remains, allowed once per substep:
///   `TOL_V = (|v1| + |v2|) · 16 ε₃₂ · S` (≈ 9e-5 m/s at S = 8).
///
/// Limitation: this checks one contact phase only (contact begins just
/// inside a substep, where the post-projection velocity equals the
/// pre-contact one), so it can pass while the inner restitution defect is
/// present; `*_at_every_contact_phase` is the phase-independent oracle.
#[test]
fn head_on_collision_conserves_momentum_and_obeys_restitution() {
    let (m1, v1, m2, v2, e) = (1.0_f64, 4.0_f64, 3.0_f64, -2.0_f64, 0.5_f64);
    let (a, b) = head_on(m1 as f32, v1 as f32, m2 as f32, v2 as f32, e as f32);

    let p0 = m1 * v1 + m2 * v2;
    let p1 = m1 * a + m2 * b;
    let tol_p = (m1 * v1.abs() + m2 * v2.abs()) * 16.0 * EPS32;
    assert!(
        (p1 - p0).abs() <= tol_p,
        "momentum {p1}, before {p0}, tol {tol_p}"
    );
    let tol_v = tol_v(v1, v2);

    let closing = v1 - v2;
    let sep = b - a;
    assert!(
        (sep - e * closing).abs() <= tol_v,
        "separation speed {sep}, closed form e·closing = {}",
        e * closing
    );

    let ke0 = 0.5 * m1 * v1 * v1 + 0.5 * m2 * v2 * v2;
    let ke1 = 0.5 * m1 * a * a + 0.5 * m2 * b * b;
    assert!(
        ke1 <= ke0 * (1.0 + 1e-6),
        "kinetic energy grew: {ke0} -> {ke1}"
    );

    let a_exp = (m1 * v1 + m2 * v2 - m2 * e * closing) / (m1 + m2);
    let b_exp = a_exp + e * closing;
    assert!(
        (a - a_exp).abs() <= tol_v && (b - b_exp).abs() <= tol_v,
        "v1' = {a} (exp {a_exp}), v2' = {b} (exp {b_exp})"
    );
}

/// Equal masses, the second at rest, `e = 1`: the velocities exchange
/// (`v1' = 0`, `v2' = v1`, from the same two laws). With `e = 0` both move
/// at `v1 / 2`. Tolerance as in the previous test.
///
/// Limitation: this checks one contact phase only (contact begins just
/// inside a substep, where the post-projection velocity equals the
/// pre-contact one), so it can pass while the inner restitution defect is
/// present; `*_at_every_contact_phase` is the phase-independent oracle.
#[test]
fn collision_with_body_at_rest_exchanges_or_shares_velocity() {
    let tol = tol_v(4.0, 0.0);
    let (a, b) = head_on(1.0, 4.0, 1.0, 0.0, 1.0);
    assert!(
        a.abs() <= tol && (b - 4.0).abs() <= tol,
        "e = 1: v1' = {a} (exp 0), v2' = {b} (exp 4)"
    );

    let (a, b) = head_on(1.0, 4.0, 1.0, 0.0, 0.0);
    assert!(
        (a - 2.0).abs() <= tol && (b - 2.0).abs() <= tol,
        "e = 0: v1' = {a}, v2' = {b} (both exp 2)"
    );
}

// ---------------------------------------------------------------------------
// 7. Resting contact and sleep
// ---------------------------------------------------------------------------

/// A sphere (radius 0.5) placed touching a static sphere of radius 100
/// (top at `y = 0`) stays at rest: its centre stays at `y = 0.5`.
///
/// Drift bound: each substep gravity moves a body at rest by `g h²` before
/// the contact pushes it back, so `|y − 0.5| ≤ g h²` (+ rounding).
///
/// It then falls asleep, and a sleeping body does not move (bit-identical
/// position over further steps). The contract does not fix *when* sleep
/// starts; 5 s is used as the horizon.
#[test]
fn body_resting_on_ground_does_not_drift_and_sleeps() {
    let dt = 1.0_f32 / 60.0;
    let s = default_substeps();
    let mut w = PhysicsWorld::new(config(-9.81, dt, s, 8));
    let ground = w.add_body(body(
        BodyKind::Static,
        Vec3::new(0.0, -100.0, 0.0),
        Vec3::ZERO,
        0.0,
        100.0,
    ));
    let b = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new(0.0, 0.5, 0.0),
        Vec3::ZERO,
        1.0,
        0.5,
    ));

    let h = h_of(&w);
    let bound = G * h * h + 0.5 * 16.0 * EPS32;
    let mut max_dev = 0.0_f64;
    for _ in 0..300 {
        w.step_fixed();
        max_dev = max_dev.max((f64::from(pos(&w, b).y) - 0.5).abs());
    }
    assert!(
        max_dev <= bound,
        "max |y - 0.5| = {max_dev}, bound g h² = {bound}"
    );
    assert_eq!(w.is_sleeping(b), Some(true), "at rest for 5 s -> asleep");
    assert_eq!(
        pos(&w, ground),
        Vec3::new(0.0, -100.0, 0.0),
        "static body never moves"
    );

    let p = pos(&w, b);
    while_steps(&mut w, 60);
    assert_eq!(
        pos(&w, b).to_array().map(f32::to_bits),
        p.to_array().map(f32::to_bits),
        "sleeping body is frozen"
    );
}

// ---------------------------------------------------------------------------
// 8. Pendulum
// ---------------------------------------------------------------------------

/// Simple pendulum: static anchor, bob (point, radius 0) on a distance joint
/// of length `L = 1 m`, released from rest at `θ0 = 0.05 rad`.
///
/// Closed form: `T = 2π √(L/g) (1 + θ0²/16 + …)` (small-angle period with
/// the first finite-amplitude correction; the next term `11 θ0⁴/3072` is
/// 2e-8 relative).
///
/// The period is the mean interval of the downward zero crossings of `x`
/// over the first 3 periods (crossing times by linear interpolation).
///
/// Tolerance:
/// - amplitude loss: a position-based solver can dissipate; the correction
///   term then lies between `θ_end²/16` and `θ0²/16`, so allow
///   `T0 (θ0² − θ_end²)/16` with `θ_end` the amplitude measured over the last
///   period;
/// - discretisation: semi-implicit Euler shortens the period by
///   `(ω h)²/24` relative; doubled to `(ω h)²/12` for the projection;
/// - interpolation: near a crossing `|x''| = ω² |x| ≤ ω² |x'| dt`, so each
///   crossing time is off by at most `ω² dt³ / 8`; two ends over `N`
///   periods give `ω² dt³ / (4 N)`.
#[test]
fn pendulum_small_amplitude_period() {
    let dt = 1.0_f32 / 60.0;
    let s = default_substeps();
    let mut w = PhysicsWorld::new(config(-9.81, dt, s, 8));
    let l = 1.0_f64;
    let th0 = 0.05_f64;
    let anchor_pos = Vec3::new(0.0, 10.0, 0.0);
    let anchor = w.add_body(body(BodyKind::Static, anchor_pos, Vec3::ZERO, 0.0, 0.0));
    let bob = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new((l * th0.sin()) as f32, (10.0 - l * th0.cos()) as f32, 0.0),
        Vec3::ZERO,
        1.0,
        0.0,
    ));
    w.add_joint(Joint::distance(anchor, bob, l as f32))
        .expect("both handles live");

    let dtf = f64::from(dt);
    let mut prev = f64::from(pos(&w, bob).x);
    let mut t = 0.0_f64;
    let mut crossings: Vec<f64> = Vec::new();
    let mut amp_last = 0.0_f64;
    for _ in 0..600 {
        w.step_fixed();
        t += dtf;
        let x = f64::from(pos(&w, bob).x);
        if crossings.len() >= 3 {
            amp_last = amp_last.max(x.abs());
        }
        if prev > 0.0 && x <= 0.0 {
            crossings.push(t - dtf + dtf * prev / (prev - x));
            if crossings.len() == 4 {
                break;
            }
        }
        prev = x;
    }
    assert_eq!(
        crossings.len(),
        4,
        "pendulum swings: crossings {crossings:?}"
    );
    let n = 3.0_f64;
    let period = (crossings[3] - crossings[0]) / n;

    let omega = (G / l).sqrt();
    let t0 = 2.0 * std::f64::consts::PI * (l / G).sqrt();
    let t_exp = t0 * (1.0 + th0 * th0 / 16.0);
    let th_end = (amp_last / l).asin();
    let h = h_of(&w);
    let tol = t0 * (th0 * th0 - th_end * th_end).max(0.0) / 16.0
        + t0 * (omega * h).powi(2) / 12.0
        + omega * omega * dtf.powi(3) / (4.0 * n);
    assert!(
        (period - t_exp).abs() <= tol,
        "period {period}, closed form {t_exp} (T0 {t0}), tol {tol}"
    );
    // Length: within one substep the bob drifts off the circle by at most
    // (g + v²/L) h² before the projection (v ≤ θ0 √(g L)); plus rounding.
    let r = f64::from((pos(&w, bob) - anchor_pos).length());
    let tol_r = (G + th0 * th0 * G) * h * h + 10.0 * 16.0 * EPS32;
    assert!(
        (r - l).abs() <= tol_r,
        "joint keeps length: {r}, tol {tol_r}"
    );
}

// ---------------------------------------------------------------------------
// 9. Fixed-step accumulator
// ---------------------------------------------------------------------------

/// A scene with motion, gravity and a bounce, used to compare runs bit for bit.
fn scene(max_steps: u32) -> (PhysicsWorld, BodyHandle, BodyHandle) {
    let mut w = PhysicsWorld::new(config(-9.81, 1.0 / 64.0, default_substeps(), max_steps));
    let ground = w.add_body(body(
        BodyKind::Static,
        Vec3::new(0.0, -100.0, 0.0),
        Vec3::ZERO,
        0.0,
        100.0,
    ));
    let mut d = body(
        BodyKind::Dynamic,
        Vec3::new(0.3, 0.6, 0.0),
        Vec3::new(1.0, 2.0, 0.5),
        1.0,
        0.5,
    );
    d.restitution = 0.5;
    let b = w.add_body(d);
    (w, ground, b)
}

fn state_bits(w: &PhysicsWorld, b: BodyHandle) -> [u32; 6] {
    let p = pos(w, b).to_array();
    let v = vel(w, b).to_array();
    [p[0], p[1], p[2], v[0], v[1], v[2]].map(f32::to_bits)
}

/// Frame-rate independence: `update(0.25)` once, `update(1/64)` 16 times and
/// `step_fixed()` 16 times give bit-identical states (`fixed_dt = 1/64` is
/// dyadic, so 0.25 is exactly 16 steps in any float type; `max_steps` is
/// raised so the single call is not clamped).
#[test]
fn accumulator_is_frame_rate_independent() {
    let (mut a, _, ba) = scene(32);
    assert_eq!(a.update(0.25), 16, "0.25 s = 16 steps of 1/64");

    let (mut b, _, bb) = scene(32);
    let mut steps = 0;
    for _ in 0..16 {
        steps += b.update(1.0 / 64.0);
    }
    assert_eq!(steps, 16);

    let (mut c, _, bc) = scene(32);
    while_steps(&mut c, 16);

    assert_eq!(
        state_bits(&a, ba),
        state_bits(&b, bb),
        "update(0.25) vs 16 × update(1/64)"
    );
    assert_eq!(
        state_bits(&a, ba),
        state_bits(&c, bc),
        "update(0.25) vs 16 × step_fixed"
    );
}

/// The remainder carries over: with `fixed_dt = 1/64`, `update(1.5/64)` runs
/// 1 step, `update(0.25/64)` runs 0, `update(0.25/64)` then reaches 2.0/64 in
/// total and runs 1 (all values dyadic, exact). The state equals 2 fixed steps.
#[test]
fn accumulator_carries_the_remainder() {
    let (mut w, _, b) = scene(8);
    let u = 1.0_f32 / 64.0;
    assert_eq!(w.update(1.5 * u), 1);
    assert_eq!(w.update(0.25 * u), 0);
    assert_eq!(w.update(0.25 * u), 1);
    let (mut r, _, rb) = scene(8);
    while_steps(&mut r, 2);
    assert_eq!(
        state_bits(&w, b),
        state_bits(&r, rb),
        "3 updates totalling 2 steps == 2 fixed steps"
    );
}

/// One `update` never runs more than `max_steps_per_update` steps:
/// `update(10/64)` with a limit of 4 returns 4 and the state equals 4 fixed
/// steps. (The time beyond the limit is dropped; see
/// `time_beyond_max_steps_is_dropped`.)
#[test]
fn accumulator_respects_max_steps_per_update() {
    let (mut w, _, b) = scene(4);
    assert_eq!(w.update(10.0 / 64.0), 4);
    let (mut r, _, rb) = scene(4);
    while_steps(&mut r, 4);
    assert_eq!(state_bits(&w, b), state_bits(&r, rb));
}

// ---------------------------------------------------------------------------
// 10. Handle stability
// ---------------------------------------------------------------------------

/// Three bodies at x = 0, 1, 2 (gravity 0, radius 0). Removing the middle
/// one (the inner world swap-removes) leaves the other two handles pointing
/// at their own bodies; the removed handle is dead everywhere and is not
/// revived by a later `add_body`, which gets a distinct handle.
#[test]
fn handles_survive_swap_remove_and_never_alias() {
    let mut w = PhysicsWorld::new(config(0.0, 1.0 / 64.0, default_substeps(), 8));
    let a = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::ZERO,
        1.0,
        0.0,
    ));
    let m = w.add_body(body(
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
    assert_eq!(w.body_count(), 3);

    assert!(w.remove_body(m));
    assert_eq!(w.body_count(), 2);
    assert_eq!(w.position(a), Some(Vec3::new(0.0, 0.0, 0.0)));
    assert_eq!(w.position(c), Some(Vec3::new(2.0, 0.0, 0.0)));
    assert!(!w.contains(m));
    assert_eq!(w.position(m), None);
    assert!(!w.remove_body(m), "second remove of the same handle");

    // The moved body still answers to its own handle when driven.
    assert!(w.apply_impulse(c, Vec3::new(0.0, 1.0, 0.0)));
    w.step_fixed();
    assert_eq!(vel(&w, a), Vec3::ZERO, "impulse on c must not reach a");
    assert!(
        (vel(&w, c).y - 1.0).abs() <= 16.0 * f32::EPSILON,
        "impulse reached c: {:?}",
        vel(&w, c)
    );

    let n = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new(5.0, 0.0, 0.0),
        Vec3::ZERO,
        1.0,
        0.0,
    ));
    assert_ne!(n, m, "a new body never reuses a removed handle");
    assert_ne!(n, a);
    assert_ne!(n, c);
    assert_eq!(
        w.position(m),
        None,
        "removed handle stays dead after a new add"
    );
    assert_eq!(w.position(n), Some(Vec3::new(5.0, 0.0, 0.0)));
    assert_eq!(w.body_count(), 3);
    assert!(w.inner_index(n).is_some() && w.inner_index(m).is_none());
}

// ---------------------------------------------------------------------------
// 11. Contact normal orientation
// ---------------------------------------------------------------------------

fn assert_normals_point_a_to_b(w: &PhysicsWorld, x: BodyHandle, y: BodyHandle) {
    let ours: Vec<_> = w
        .contacts()
        .iter()
        .filter(|c| (c.body_a == x && c.body_b == y) || (c.body_a == y && c.body_b == x))
        .collect();
    assert!(!ours.is_empty(), "the pair touched during the last step");
    for c in ours {
        let d = pos(w, c.body_b) - pos(w, c.body_a);
        let n = c.normal;
        assert!((n.length() - 1.0).abs() <= 1e-5, "unit normal: {n:?}");
        assert!(
            n.dot(d) > 0.0,
            "normal {n:?} must point from body_a to body_b ({d:?})"
        );
        assert!(c.penetration >= 0.0, "penetration {} >= 0", c.penetration);
    }
}

/// Contacts report `normal` from `body_a` to `body_b`:
/// `normal · (pos_b − pos_a) > 0`. Checked for an overlapping dynamic pair
/// (in both insertion orders) and for a sphere resting on static ground
/// (a contact present in every substep).
#[test]
fn contact_normal_points_from_a_to_b() {
    for flip in [false, true] {
        let mut w = PhysicsWorld::new(config(0.0, 1.0 / 64.0, default_substeps(), 8));
        let p = body(
            BodyKind::Dynamic,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::ZERO,
            1.0,
            0.5,
        );
        let q = body(
            BodyKind::Dynamic,
            Vec3::new(0.8, 0.1, 0.0),
            Vec3::ZERO,
            1.0,
            0.5,
        );
        let (x, y) = if flip {
            let y = w.add_body(q);
            (w.add_body(p), y)
        } else {
            let x = w.add_body(p);
            (x, w.add_body(q))
        };
        w.step_fixed();
        assert_normals_point_a_to_b(&w, x, y);
    }

    let mut w = PhysicsWorld::new(config(-9.81, 1.0 / 64.0, default_substeps(), 8));
    let b = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new(0.0, 0.5, 0.0),
        Vec3::ZERO,
        1.0,
        0.5,
    ));
    let g = w.add_body(body(
        BodyKind::Static,
        Vec3::new(0.0, -100.0, 0.0),
        Vec3::ZERO,
        0.0,
        100.0,
    ));
    while_steps(&mut w, 3);
    assert_normals_point_a_to_b(&w, b, g);
}

// ---------------------------------------------------------------------------
// 12. Contract additions: material average, sleep timing, defaults
// ---------------------------------------------------------------------------

/// Contract: restitution of two bodies combines by the average. With
/// `e_a = 0.2`, `e_b = 0.8` the pair behaves as `e = 0.5`, so (same scene and
/// laws as the head-on test) `v2' − v1' = 0.5 (v1 − v2)` and momentum is
/// kept. A product rule would give 0.16, min 0.2, max 0.8.
///
/// Tolerance: as in the head-on test (`tol_v`, rounding only).
///
/// Note: the inner `alice_physics` velocity pass applies restitution to the
/// post-projection velocity `−gap_prev / h`, so the separation depends on
/// the substep phase at which contact begins; that defect of the inner world
/// is caught by `average_restitution_holds_at_every_contact_phase`.
///
/// Limitation: this checks one contact phase only (contact begins just
/// inside a substep, where the post-projection velocity equals the
/// pre-contact one), so it can pass while the inner restitution defect is
/// present; `*_at_every_contact_phase` is the phase-independent oracle.
#[test]
fn restitution_of_a_pair_is_the_average() {
    let (m1, v1, m2, v2) = (1.0_f64, 4.0_f64, 3.0_f64, -2.0_f64);
    let (ea, eb) = (0.2_f64, 0.8_f64);
    let (a, b) = head_on_pair(
        m1 as f32, v1 as f32, m2 as f32, v2 as f32, ea as f32, eb as f32,
    );
    let e = 0.5 * (ea + eb);
    let tol = tol_v(v1, v2);
    let p0 = m1 * v1 + m2 * v2;
    assert!(
        (m1 * a + m2 * b - p0).abs() <= (m1 * v1.abs() + m2 * v2.abs()) * 16.0 * EPS32,
        "momentum kept"
    );
    let sep = b - a;
    assert!(
        (sep - e * (v1 - v2)).abs() <= tol,
        "separation {sep}, closed form mean(e)·closing = {}",
        e * (v1 - v2)
    );
}

/// Contract: a body at rest falls asleep within 60 fixed steps. The ball is
/// placed touching the ground with zero velocity (at rest from step 0), so
/// after exactly 60 `step_fixed` it reports sleeping.
#[test]
fn body_at_rest_sleeps_within_60_fixed_steps() {
    let mut w = PhysicsWorld::new(config(-9.81, 1.0 / 60.0, default_substeps(), 8));
    w.add_body(body(
        BodyKind::Static,
        Vec3::new(0.0, -100.0, 0.0),
        Vec3::ZERO,
        0.0,
        100.0,
    ));
    let b = w.add_body(body(
        BodyKind::Dynamic,
        Vec3::new(0.0, 0.5, 0.0),
        Vec3::ZERO,
        1.0,
        0.5,
    ));
    while_steps(&mut w, 60);
    assert_eq!(
        w.is_sleeping(b),
        Some(true),
        "asleep after 60 fixed steps at rest"
    );
}

/// Contract: `PhysicsConfig::default()` is gravity `(0, -9.81, 0)`,
/// `fixed_dt = 1/60`, `substeps = 4`, `max_steps_per_update = 8`,
/// `broadphase = DynamicTree`
/// (`1.0_f32 / 60.0` is the f32 the contract value denotes).
#[test]
fn default_config_values() {
    let c = PhysicsConfig::default();
    assert_eq!(c.gravity, Vec3::new(0.0, -9.81, 0.0));
    assert_eq!(c.fixed_dt.to_bits(), (1.0_f32 / 60.0).to_bits());
    assert_eq!(c.substeps, 4);
    assert_eq!(c.max_steps_per_update, 8);
    assert_eq!(c.broadphase, Broadphase::DynamicTree);
    let w = PhysicsWorld::new(c);
    assert_eq!(w.config(), c, "config() returns what new() was given");
}

/// Contract: when one `update` reaches `max_steps_per_update` the time left
/// over is dropped. `update(10/64)` with a limit of 4 runs 4 steps; the next
/// `update(0)` runs 0 (no catch-up), and `update(1/64)` runs exactly 1.
#[test]
fn time_beyond_max_steps_is_dropped() {
    let (mut w, _, _) = scene(4);
    assert_eq!(w.update(10.0 / 64.0), 4);
    assert_eq!(w.update(0.0), 0, "left-over 6/64 s is dropped");
    assert_eq!(w.update(1.0 / 64.0), 1, "a normal frame runs one step");
}

// ---------------------------------------------------------------------------
// 13. Restitution over the contact phase
// ---------------------------------------------------------------------------

/// Sweep the starting gap over one substep of approach: the second body is
/// moved by `offset = k/16 · (v1 − v2) h` for `k = 0..16` (`h = dt / 8`,
/// `dt = 1/60`), so contact begins at 16 different points inside a substep.
/// Newton's law and momentum conservation do not depend on where inside a
/// substep the contact begins, so every phase must give the closed form
/// (tolerance `2 tol_v`: the separation is the difference of two velocities,
/// each rounded within `tol_v` as in the single-phase tests).
///
/// Red while the inner `alice_physics` `update_velocities` applies
/// restitution to the post-projection velocity `−gap_prev / h` instead of
/// the pre-solve velocity: the separation then scales with the substep
/// phase (`≈ e (v1 − v2) k/16` for `k ≥ 1`). This is a defect of the inner
/// world, not of the adapter.
///
/// Returns the phases that miss, as `(k, separation, expected)`.
fn restitution_phase_misses(
    m1: f64,
    v1: f64,
    m2: f64,
    v2: f64,
    ea: f64,
    eb: f64,
) -> Vec<(u32, f64, f64)> {
    let h = f64::from(1.0_f32 / 60.0) / f64::from(default_substeps());
    let closing = v1 - v2;
    let e = 0.5 * (ea + eb);
    let expected = e * closing;
    let tol = 2.0 * tol_v(v1, v2);
    let mut misses = Vec::new();
    for k in 0..16_u32 {
        let offset = f64::from(k) / 16.0 * closing * h;
        let (a, b) = head_on_offset(
            m1 as f32,
            v1 as f32,
            m2 as f32,
            v2 as f32,
            ea as f32,
            eb as f32,
            offset as f32,
        );
        let p_err = (m1 * a + m2 * b - (m1 * v1 + m2 * v2)).abs();
        assert!(
            p_err <= (m1 * v1.abs() + m2 * v2.abs()) * 16.0 * EPS32,
            "phase {k}: momentum error {p_err}"
        );
        let sep = b - a;
        if (sep - expected).abs() > tol {
            misses.push((k, sep, expected));
        }
    }
    misses
}

/// Head-on, `m = 1, 3`, `v = +4, −2`, `e = 0.5`: separation `3.0` at every
/// contact phase.
///
/// The single-phase `head_on_collision_conserves_momentum_and_obeys_restitution`
/// can pass at a phase where the inner defect (restitution applied to the
/// post-projection velocity `−gap_prev / h`) happens not to show; this sweep
/// is the oracle that does not depend on the phase.
#[test]
fn head_on_restitution_holds_at_every_contact_phase() {
    let misses = restitution_phase_misses(1.0, 4.0, 3.0, -2.0, 0.5, 0.5);
    assert!(
        misses.is_empty(),
        "phases (k, separation, closed form) that miss: {misses:?}"
    );
}

/// Equal masses, second at rest, `e = 1`: separation `4.0` (velocity
/// exchange) at every contact phase.
#[test]
fn collision_with_body_at_rest_holds_at_every_contact_phase() {
    let misses = restitution_phase_misses(1.0, 4.0, 1.0, 0.0, 1.0, 1.0);
    assert!(
        misses.is_empty(),
        "phases (k, separation, closed form) that miss: {misses:?}"
    );
}

/// `e_a = 0.2`, `e_b = 0.8` (average 0.5): separation `3.0` at every phase.
#[test]
fn average_restitution_holds_at_every_contact_phase() {
    let misses = restitution_phase_misses(1.0, 4.0, 3.0, -2.0, 0.2, 0.8);
    assert!(
        misses.is_empty(),
        "phases (k, separation, closed form) that miss: {misses:?}"
    );
}

// ---------------------------------------------------------------------------
// 14. Broad-phase choice does not change the result
// ---------------------------------------------------------------------------

/// A broad-phase only decides which pairs reach the narrow phase; it must
/// not change what the solver computes (`alice_physics` documents the
/// broad-phases as bit-identical). The same scene — 27 spheres of radius
/// 0.5 dropped in a 3×3×3 lattice with 0.9 m spacing (overlapping
/// neighbours) onto a static ground sphere, with restitution and friction —
/// is run 180 fixed steps with `Bvh` and with `DynamicTree`, at the default
/// substep count and at 8. Every body's position, rotation, velocity and
/// angular velocity must match bit for bit, and so must the sleep flags.
#[test]
fn broadphase_choice_is_bit_identical() {
    fn run(bp: Broadphase, substeps: u32) -> Vec<[u32; 14]> {
        let mut c = config(-9.81, 1.0 / 60.0, substeps, 8);
        c.broadphase = bp;
        let mut w = PhysicsWorld::new(c);
        w.add_body(body(
            BodyKind::Static,
            Vec3::new(0.0, -100.0, 0.0),
            Vec3::ZERO,
            0.0,
            100.0,
        ));
        let mut hs = Vec::new();
        for i in 0..3 {
            for j in 0..3 {
                for k in 0..3 {
                    let p = Vec3::new(i as f32 * 0.9, 0.6 + j as f32 * 0.9, k as f32 * 0.9);
                    let mut d = body(BodyKind::Dynamic, p, Vec3::new(0.3, 0.0, -0.2), 1.0, 0.5);
                    d.restitution = 0.3;
                    d.friction = 0.5;
                    hs.push(w.add_body(d));
                }
            }
        }
        while_steps(&mut w, 180);
        hs.iter()
            .map(|&h| {
                let p = pos(&w, h).to_array();
                let q = w.rotation(h).expect("live").to_array();
                let v = vel(&w, h).to_array();
                let a = w.angular_velocity(h).expect("live").to_array();
                let sl = u32::from(w.is_sleeping(h).expect("live"));
                [
                    p[0].to_bits(),
                    p[1].to_bits(),
                    p[2].to_bits(),
                    q[0].to_bits(),
                    q[1].to_bits(),
                    q[2].to_bits(),
                    q[3].to_bits(),
                    v[0].to_bits(),
                    v[1].to_bits(),
                    v[2].to_bits(),
                    a[0].to_bits(),
                    a[1].to_bits(),
                    a[2].to_bits(),
                    sl,
                ]
            })
            .collect()
    }
    for s in [default_substeps(), 8] {
        let bvh = run(Broadphase::Bvh, s);
        let tree = run(Broadphase::DynamicTree, s);
        assert_eq!(bvh.len(), 27);
        for (i, (x, y)) in bvh.iter().zip(&tree).enumerate() {
            assert_eq!(
                x, y,
                "substeps {s}: body {i} differs between Bvh and DynamicTree"
            );
        }
    }
}
