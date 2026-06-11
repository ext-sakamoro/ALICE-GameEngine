//! Real-time 3D action combat — orthogonal to the turn-based [`crate::battle`]
//! runner. Pieces:
//!
//! - [`Hitbox`] / [`Hurtbox`] — sphere or capsule volumes attached to an
//!   entity, used by [`resolve_hits`] to find overlaps each tick
//! - [`ComboSystem`] — input-window buffered move tree (input sequence →
//!   move name, with branching)
//! - [`LockOn`] — auto-select the best target inside a cone + distance,
//!   with smooth blend
//! - [`HitStop`] — global time-scale freeze on impact for "weighty" hits
//! - [`HitEvent`] — emitted by [`resolve_hits`], consumable by VFX /
//!   audio / damage application
//!
//! Designed for action-RPG / character-action games (Devil May Cry,
//! Souls-likes, Sekiro, Octopath's optional real-time mode).

use crate::math::Vec3;
use std::collections::VecDeque;

/// A spherical or capsule volume that *deals* damage when overlapping a
/// [`Hurtbox`].
#[derive(Debug, Clone)]
pub struct Hitbox {
    pub id: u32,
    /// Entity that owns the hitbox (attacker id).
    pub owner: u32,
    pub shape: ColliderShape,
    /// Move/ability name that produced this hit, for `HitEvent` debug.
    pub source: String,
    /// Damage to apply if it lands.
    pub damage: f32,
    /// Knockback impulse in world units.
    pub knockback: f32,
    /// Hitstop frames to enforce on impact (`0` = no stop).
    pub hitstop_frames: u32,
    /// True if the hitbox should only register once per
    /// [`resolve_hits`] pass per hurtbox owner.
    pub one_hit_per_target: bool,
    /// Internal tracker for `one_hit_per_target`.
    consumed_targets: Vec<u32>,
}

impl Hitbox {
    #[must_use]
    pub fn new(id: u32, owner: u32, shape: ColliderShape, source: impl Into<String>) -> Self {
        Self {
            id,
            owner,
            shape,
            source: source.into(),
            damage: 0.0,
            knockback: 0.0,
            hitstop_frames: 0,
            one_hit_per_target: true,
            consumed_targets: Vec::new(),
        }
    }
}

/// A spherical or capsule volume that *receives* damage.
#[derive(Debug, Clone)]
pub struct Hurtbox {
    pub id: u32,
    /// Entity that owns the hurtbox (target id).
    pub owner: u32,
    pub shape: ColliderShape,
    /// Optional invincibility timer in frames; while >0, no hits register.
    pub invuln_frames: u32,
}

impl Hurtbox {
    #[must_use]
    pub const fn new(id: u32, owner: u32, shape: ColliderShape) -> Self {
        Self {
            id,
            owner,
            shape,
            invuln_frames: 0,
        }
    }
}

/// Sphere or capsule volume.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColliderShape {
    Sphere {
        center: Vec3,
        radius: f32,
    },
    /// Capsule between `a` and `b` with `radius` around the axis.
    Capsule {
        a: Vec3,
        b: Vec3,
        radius: f32,
    },
}

impl ColliderShape {
    /// True if the two shapes overlap.
    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Sphere {
                    center: a,
                    radius: ra,
                },
                Self::Sphere {
                    center: b,
                    radius: rb,
                },
            ) => a.distance(*b) <= ra + rb,
            (Self::Sphere { center, radius }, Self::Capsule { a, b, radius: cr })
            | (Self::Capsule { a, b, radius: cr }, Self::Sphere { center, radius }) => {
                let d = closest_point_on_segment(*center, *a, *b).distance(*center);
                d <= radius + cr
            }
            (
                Self::Capsule {
                    a: a1,
                    b: b1,
                    radius: r1,
                },
                Self::Capsule {
                    a: a2,
                    b: b2,
                    radius: r2,
                },
            ) => segment_segment_distance(*a1, *b1, *a2, *b2) <= r1 + r2,
        }
    }
}

fn closest_point_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    if len2 < 1e-8 {
        return a;
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    a + ab * t
}

fn segment_segment_distance(a1: Vec3, b1: Vec3, a2: Vec3, b2: Vec3) -> f32 {
    // Approximate via mid-point closest-point iteration — good enough for
    // gameplay-grade combat. Game-engine math is geometric, not algebraic.
    let mut p = (a1 + b1) * 0.5;
    let mut q = (a2 + b2) * 0.5;
    for _ in 0..4 {
        p = closest_point_on_segment(q, a1, b1);
        q = closest_point_on_segment(p, a2, b2);
    }
    p.distance(q)
}

/// A registered "hit" from [`resolve_hits`]. Game code consumes these to
/// apply damage, trigger SFX, etc.
#[derive(Debug, Clone)]
pub struct HitEvent {
    pub hitbox_id: u32,
    pub hurtbox_id: u32,
    pub attacker: u32,
    pub target: u32,
    pub damage: f32,
    pub knockback: f32,
    pub hitstop_frames: u32,
    pub source: String,
}

/// Find every (hitbox, hurtbox) pair that overlaps this frame, respecting
/// invincibility and `one_hit_per_target`. Updates hitboxes' consumed
/// list, decrements hurtbox invuln timers by 1.
pub fn resolve_hits(hitboxes: &mut [Hitbox], hurtboxes: &mut [Hurtbox]) -> Vec<HitEvent> {
    let mut events = Vec::new();
    for hit in hitboxes.iter_mut() {
        for hurt in hurtboxes.iter_mut() {
            if hit.owner == hurt.owner {
                continue;
            }
            if hurt.invuln_frames > 0 {
                continue;
            }
            if hit.one_hit_per_target && hit.consumed_targets.contains(&hurt.owner) {
                continue;
            }
            if !hit.shape.intersects(&hurt.shape) {
                continue;
            }
            events.push(HitEvent {
                hitbox_id: hit.id,
                hurtbox_id: hurt.id,
                attacker: hit.owner,
                target: hurt.owner,
                damage: hit.damage,
                knockback: hit.knockback,
                hitstop_frames: hit.hitstop_frames,
                source: hit.source.clone(),
            });
            if hit.one_hit_per_target {
                hit.consumed_targets.push(hurt.owner);
            }
        }
    }
    // Tick invuln timers.
    for hurt in hurtboxes.iter_mut() {
        hurt.invuln_frames = hurt.invuln_frames.saturating_sub(1);
    }
    events
}

// ---------------------------------------------------------------------------
// ComboSystem — input-buffered move tree
// ---------------------------------------------------------------------------

/// A single move in a [`ComboTree`].
#[derive(Debug, Clone)]
pub struct ComboMove {
    pub name: String,
    /// The input that completes this move (e.g. "light", "heavy", "dodge").
    pub input: String,
    /// Active frame window — `0..=window_frames` after the previous move
    /// finished, during which this input continues the combo.
    pub window_frames: u32,
    /// Optional next moves; if multiple share the same input, the first
    /// match wins (define more-specific branches earlier).
    pub follow_ups: Vec<Self>,
}

impl ComboMove {
    #[must_use]
    pub fn leaf(name: impl Into<String>, input: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            input: input.into(),
            window_frames: 30,
            follow_ups: Vec::new(),
        }
    }

    #[must_use]
    pub const fn with_window(mut self, window_frames: u32) -> Self {
        self.window_frames = window_frames;
        self
    }

    pub fn follow_up(&mut self, m: Self) -> &mut Self {
        self.follow_ups.push(m);
        self
    }
}

/// Stateful combo tracker. The host pushes inputs each frame; the system
/// returns the resolved move (or `None` for buffered/unmatched).
pub struct ComboSystem {
    root_moves: Vec<ComboMove>,
    pending_path: Vec<usize>, // indices into the tree (for debugging)
    current: Option<ComboMove>,
    frames_since_last: u32,
    input_buffer: VecDeque<String>,
}

impl ComboSystem {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            root_moves: Vec::new(),
            pending_path: Vec::new(),
            current: None,
            frames_since_last: 0,
            input_buffer: VecDeque::new(),
        }
    }

    /// Add a top-level combo starter.
    pub fn add_root(&mut self, m: ComboMove) -> &mut Self {
        self.root_moves.push(m);
        self
    }

    /// Push an input (e.g. from `ActionMap`). Returns the move that
    /// activated this frame, if any.
    pub fn input(&mut self, action: impl Into<String>) -> Option<String> {
        self.input_buffer.push_back(action.into());
        self.try_resolve()
    }

    /// Advance one frame — drops the input buffer if it ages past the
    /// current move's window.
    pub fn step(&mut self) {
        self.frames_since_last += 1;
        if let Some(cur) = &self.current {
            if self.frames_since_last > cur.window_frames {
                // Combo expired — reset.
                self.current = None;
                self.pending_path.clear();
                self.input_buffer.clear();
            }
        }
    }

    fn try_resolve(&mut self) -> Option<String> {
        let input = self.input_buffer.pop_front()?;

        // If a combo is active, try its follow-ups first.
        if let Some(cur) = self.current.clone() {
            if self.frames_since_last <= cur.window_frames {
                for fu in &cur.follow_ups {
                    if fu.input == input {
                        let name = fu.name.clone();
                        self.current = Some(fu.clone());
                        self.frames_since_last = 0;
                        return Some(name);
                    }
                }
            }
        }
        // Try roots.
        for root in &self.root_moves {
            if root.input == input {
                let name = root.name.clone();
                self.current = Some(root.clone());
                self.frames_since_last = 0;
                return Some(name);
            }
        }
        None
    }

    /// Current active move name (if any).
    #[must_use]
    pub fn current_move(&self) -> Option<&str> {
        self.current.as_ref().map(|c| c.name.as_str())
    }
}

impl Default for ComboSystem {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// LockOn — auto-target inside cone + distance
// ---------------------------------------------------------------------------

/// A potential lock-on target.
#[derive(Debug, Clone, Copy)]
pub struct LockOnCandidate {
    pub entity: u32,
    pub position: Vec3,
}

/// Lock-on selector — picks the target with the best score inside an
/// angular cone in front of the camera.
#[derive(Debug, Clone)]
pub struct LockOn {
    pub max_distance: f32,
    /// Half-angle of the cone in radians (`0.523 ≈ 30°`).
    pub cone_half_angle: f32,
    pub current: Option<u32>,
}

impl LockOn {
    #[must_use]
    pub const fn new(max_distance: f32, cone_half_angle: f32) -> Self {
        Self {
            max_distance,
            cone_half_angle,
            current: None,
        }
    }

    /// Pick the best candidate. Score = `dot(forward, to_candidate) /
    /// distance` — closer + more centered wins.
    pub fn acquire(
        &mut self,
        viewer: Vec3,
        forward: Vec3,
        candidates: &[LockOnCandidate],
    ) -> Option<u32> {
        let mut best: Option<(f32, u32)> = None;
        let cos_thr = self.cone_half_angle.cos();
        for c in candidates {
            let to = c.position - viewer;
            let d = to.length();
            if d > self.max_distance || d < 1e-3 {
                continue;
            }
            let dir = Vec3::new(to.x() / d, to.y() / d, to.z() / d);
            let dot = forward.dot(dir);
            if dot < cos_thr {
                continue;
            }
            let score = dot / d.max(1e-3);
            if best.is_none_or(|(b, _)| score > b) {
                best = Some((score, c.entity));
            }
        }
        self.current = best.map(|(_, id)| id);
        self.current
    }

    pub const fn release(&mut self) {
        self.current = None;
    }
}

// ---------------------------------------------------------------------------
// HitStop — global tick-scale freeze
// ---------------------------------------------------------------------------

/// Time-scale freeze applied on impact, used to give attacks "weight"
/// (Sekiro / DMC style).
#[derive(Debug, Clone, Copy, Default)]
pub struct HitStop {
    pub remaining_frames: u32,
}

impl HitStop {
    /// Trigger / extend a hitstop. If a longer stop is already active,
    /// the larger value wins (no shortening).
    pub fn trigger(&mut self, frames: u32) {
        self.remaining_frames = self.remaining_frames.max(frames);
    }

    pub const fn step(&mut self) {
        self.remaining_frames = self.remaining_frames.saturating_sub(1);
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.remaining_frames > 0
    }

    /// Returns the effective time-scale this frame: `0.0` while a stop is
    /// active, `1.0` otherwise.
    #[must_use]
    pub const fn time_scale(&self) -> f32 {
        if self.is_active() {
            0.0
        } else {
            1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sphere(x: f32, y: f32, z: f32, r: f32) -> ColliderShape {
        ColliderShape::Sphere {
            center: Vec3::new(x, y, z),
            radius: r,
        }
    }

    // ---- Shape intersection -------------------------------------------

    #[test]
    fn sphere_sphere_overlap() {
        let a = sphere(0.0, 0.0, 0.0, 1.0);
        let b = sphere(1.5, 0.0, 0.0, 1.0);
        let c = sphere(5.0, 0.0, 0.0, 1.0);
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }

    #[test]
    fn sphere_capsule_overlap() {
        let s = sphere(0.0, 0.0, 0.0, 0.4);
        let cap = ColliderShape::Capsule {
            a: Vec3::new(-1.0, 0.5, 0.0),
            b: Vec3::new(1.0, 0.5, 0.0),
            radius: 0.2,
        };
        assert!(s.intersects(&cap));
        let far = ColliderShape::Capsule {
            a: Vec3::new(-1.0, 5.0, 0.0),
            b: Vec3::new(1.0, 5.0, 0.0),
            radius: 0.2,
        };
        assert!(!s.intersects(&far));
    }

    // ---- resolve_hits ------------------------------------------------

    #[test]
    fn hit_register_overlapping() {
        let mut hits = vec![{
            let mut h = Hitbox::new(1, 100, sphere(0.0, 0.0, 0.0, 1.0), "punch");
            h.damage = 10.0;
            h.hitstop_frames = 5;
            h
        }];
        let mut hurts = vec![Hurtbox::new(2, 200, sphere(0.5, 0.0, 0.0, 1.0))];
        let events = resolve_hits(&mut hits, &mut hurts);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].damage, 10.0);
        assert_eq!(events[0].attacker, 100);
        assert_eq!(events[0].target, 200);
    }

    #[test]
    fn hit_skipped_for_same_owner() {
        let mut hits = vec![Hitbox::new(1, 100, sphere(0.0, 0.0, 0.0, 1.0), "self")];
        let mut hurts = vec![Hurtbox::new(2, 100, sphere(0.5, 0.0, 0.0, 1.0))];
        assert!(resolve_hits(&mut hits, &mut hurts).is_empty());
    }

    #[test]
    fn hit_skipped_for_invuln() {
        let mut hits = vec![Hitbox::new(1, 100, sphere(0.0, 0.0, 0.0, 1.0), "punch")];
        let mut hurts = vec![{
            let mut h = Hurtbox::new(2, 200, sphere(0.5, 0.0, 0.0, 1.0));
            h.invuln_frames = 10;
            h
        }];
        assert!(resolve_hits(&mut hits, &mut hurts).is_empty());
        // Invuln decremented.
        assert_eq!(hurts[0].invuln_frames, 9);
    }

    #[test]
    fn one_hit_per_target_dedupes() {
        let mut hits = vec![{
            let mut h = Hitbox::new(1, 100, sphere(0.0, 0.0, 0.0, 1.0), "swipe");
            h.damage = 1.0;
            h.one_hit_per_target = true;
            h
        }];
        let mut hurts = vec![Hurtbox::new(2, 200, sphere(0.5, 0.0, 0.0, 1.0))];
        let e1 = resolve_hits(&mut hits, &mut hurts);
        let e2 = resolve_hits(&mut hits, &mut hurts);
        assert_eq!(e1.len(), 1);
        assert!(e2.is_empty());
    }

    // ---- ComboSystem ------------------------------------------------

    #[test]
    fn combo_root_input_activates_root_move() {
        let mut sys = ComboSystem::new();
        sys.add_root(ComboMove::leaf("light_1", "light"));
        let m = sys.input("light");
        assert_eq!(m.as_deref(), Some("light_1"));
        assert_eq!(sys.current_move(), Some("light_1"));
    }

    #[test]
    fn combo_follow_up_within_window() {
        let mut sys = ComboSystem::new();
        let mut root = ComboMove::leaf("light_1", "light").with_window(30);
        root.follow_up(ComboMove::leaf("light_2", "light").with_window(30));
        sys.add_root(root);
        sys.input("light");
        // Within window — should chain.
        let m = sys.input("light");
        assert_eq!(m.as_deref(), Some("light_2"));
    }

    #[test]
    fn combo_window_expires() {
        let mut sys = ComboSystem::new();
        let mut root = ComboMove::leaf("light_1", "light").with_window(3);
        root.follow_up(ComboMove::leaf("light_2", "light"));
        sys.add_root(root);
        sys.input("light");
        for _ in 0..10 {
            sys.step();
        }
        // Window expired — falls back to root again.
        let m = sys.input("light");
        assert_eq!(m.as_deref(), Some("light_1"));
    }

    // ---- LockOn -----------------------------------------------------

    #[test]
    fn lock_on_picks_in_front() {
        let mut lock = LockOn::new(20.0, 0.6);
        let cands = vec![
            LockOnCandidate {
                entity: 1,
                position: Vec3::new(0.0, 0.0, 5.0), // straight ahead
            },
            LockOnCandidate {
                entity: 2,
                position: Vec3::new(0.0, 0.0, -5.0), // behind
            },
        ];
        let picked = lock.acquire(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0), &cands);
        assert_eq!(picked, Some(1));
    }

    #[test]
    fn lock_on_outside_distance_skipped() {
        let mut lock = LockOn::new(3.0, 1.0);
        let cands = vec![LockOnCandidate {
            entity: 1,
            position: Vec3::new(0.0, 0.0, 10.0),
        }];
        assert!(lock
            .acquire(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0), &cands)
            .is_none());
    }

    #[test]
    fn lock_on_outside_cone_skipped() {
        let mut lock = LockOn::new(20.0, 0.1); // very narrow
        let cands = vec![LockOnCandidate {
            entity: 1,
            position: Vec3::new(5.0, 0.0, 1.0), // mostly sideways
        }];
        assert!(lock
            .acquire(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0), &cands)
            .is_none());
    }

    // ---- HitStop ----------------------------------------------------

    #[test]
    fn hitstop_freezes_then_recovers() {
        let mut hs = HitStop::default();
        hs.trigger(3);
        assert!(hs.is_active());
        assert_eq!(hs.time_scale(), 0.0);
        for _ in 0..3 {
            hs.step();
        }
        assert!(!hs.is_active());
        assert_eq!(hs.time_scale(), 1.0);
    }

    #[test]
    fn hitstop_takes_max_when_triggered_twice() {
        let mut hs = HitStop::default();
        hs.trigger(3);
        hs.trigger(8); // larger wins
        assert_eq!(hs.remaining_frames, 8);
        hs.trigger(2); // shorter — ignored
        assert_eq!(hs.remaining_frames, 8);
    }
}
