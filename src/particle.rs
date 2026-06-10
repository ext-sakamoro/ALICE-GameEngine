//! GPU-ready particle system with CPU fallback.
//!
//! Unlike Fyrox's CPU-only particles, this module is designed for GPU
//! compute shader dispatch when the `gpu` feature is enabled, with a
//! CPU simulation path for headless/test use.

use crate::math::{Color, Vec3};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Particle
// ---------------------------------------------------------------------------

/// A single particle.
#[derive(Debug, Clone, Copy)]
pub struct Particle {
    pub position: Vec3,
    pub velocity: Vec3,
    pub color: Color,
    pub size: f32,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub alive: bool,
}

impl Particle {
    pub const DEAD: Self = Self {
        position: Vec3::ZERO,
        velocity: Vec3::ZERO,
        color: Color::TRANSPARENT,
        size: 0.0,
        lifetime: 0.0,
        max_lifetime: 0.0,
        alive: false,
    };

    /// Returns how far through its life this particle is (0.0 = born, 1.0 = dead).
    #[inline]
    #[must_use]
    pub fn life_ratio(&self) -> f32 {
        if self.max_lifetime <= 0.0 {
            return 1.0;
        }
        (self.lifetime / self.max_lifetime).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// EmitterShape
// ---------------------------------------------------------------------------

/// Shape from which particles are emitted.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum EmitterShape {
    #[default]
    Point,
    Sphere {
        radius: f32,
    },
    Box {
        half_extents: Vec3,
    },
    Cone {
        radius: f32,
        angle: f32,
    },
}

// ---------------------------------------------------------------------------
// EmitterConfig
// ---------------------------------------------------------------------------

/// Configuration for a particle emitter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmitterConfig {
    pub max_particles: u32,
    pub emit_rate: f32,
    pub lifetime_min: f32,
    pub lifetime_max: f32,
    pub speed_min: f32,
    pub speed_max: f32,
    pub size_start: f32,
    pub size_end: f32,
    pub color_start: Color,
    pub color_end: Color,
    pub gravity: Vec3,
    pub shape: EmitterShape,
    pub world_space: bool,
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            max_particles: 1000,
            emit_rate: 50.0,
            lifetime_min: 1.0,
            lifetime_max: 2.0,
            speed_min: 1.0,
            speed_max: 5.0,
            size_start: 0.1,
            size_end: 0.0,
            color_start: Color::WHITE,
            color_end: Color::TRANSPARENT,
            gravity: Vec3::new(0.0, -9.81, 0.0),
            shape: EmitterShape::Point,
            world_space: true,
        }
    }
}

// ---------------------------------------------------------------------------
// ParticleEmitter (CPU path)
// ---------------------------------------------------------------------------

/// CPU particle emitter for simulation and testing.
pub struct ParticleEmitter {
    pub config: EmitterConfig,
    pub particles: Vec<Particle>,
    pub position: Vec3,
    emit_accumulator: f32,
    alive_count: u32,
    seed: u32,
}

impl ParticleEmitter {
    #[must_use]
    pub fn new(config: EmitterConfig) -> Self {
        let max = config.max_particles as usize;
        Self {
            config,
            particles: vec![Particle::DEAD; max],
            position: Vec3::ZERO,
            emit_accumulator: 0.0,
            alive_count: 0,
            seed: 12345,
        }
    }

    /// Advances the simulation by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        // Update existing particles
        self.alive_count = 0;
        for p in &mut self.particles {
            if !p.alive {
                continue;
            }
            p.lifetime += dt;
            if p.lifetime >= p.max_lifetime {
                p.alive = false;
                continue;
            }
            p.velocity = p.velocity + self.config.gravity * dt;
            p.position = p.position + p.velocity * dt;

            let t = p.life_ratio();
            p.color = self.config.color_start.lerp(self.config.color_end, t);
            p.size =
                (self.config.size_end - self.config.size_start).mul_add(t, self.config.size_start);
            self.alive_count += 1;
        }

        // Emit new particles
        self.emit_accumulator += self.config.emit_rate * dt;
        #[allow(clippy::while_float)]
        while self.emit_accumulator >= 1.0 {
            self.emit_accumulator -= 1.0;
            self.emit_one();
        }
    }

    fn emit_one(&mut self) {
        for p in &mut self.particles {
            if p.alive {
                continue;
            }
            self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let r01 = (self.seed >> 16) as f32 / 65535.0;

            let speed =
                self.config.speed_min + r01 * (self.config.speed_max - self.config.speed_min);
            let lifetime = self.config.lifetime_min
                + r01 * (self.config.lifetime_max - self.config.lifetime_min);

            let dir = match self.config.shape {
                EmitterShape::Point => Vec3::new(
                    r01 * 2.0 - 1.0,
                    (r01 * std::f32::consts::PI).cos(),
                    r01 * 2.0 - 1.0,
                )
                .normalize(),
                EmitterShape::Sphere { .. }
                | EmitterShape::Box { .. }
                | EmitterShape::Cone { .. } => Vec3::Y,
            };

            *p = Particle {
                position: self.position,
                velocity: dir * speed,
                color: self.config.color_start,
                size: self.config.size_start,
                lifetime: 0.0,
                max_lifetime: lifetime,
                alive: true,
            };
            self.alive_count += 1;
            return;
        }
    }

    /// Returns the number of alive particles.
    #[must_use]
    pub const fn alive_count(&self) -> u32 {
        self.alive_count
    }

    /// Kills all particles.
    pub fn clear(&mut self) {
        for p in &mut self.particles {
            p.alive = false;
        }
        self.alive_count = 0;
        self.emit_accumulator = 0.0;
    }
}

// ---------------------------------------------------------------------------
// Curl noise force field — divergence-free flow for organic particle motion
// ---------------------------------------------------------------------------

/// Hash-based scalar value-noise in 3D, returns roughly `[-1, 1]`.
#[inline]
fn vnoise3(x: f32, y: f32, z: f32) -> f32 {
    let ix = x.floor();
    let iy = y.floor();
    let iz = z.floor();
    let h = ((ix * 12.9898) + (iy * 78.233) + (iz * 37.719)).sin() * 43_758.547;
    let v = h - h.floor();
    v * 2.0 - 1.0
}

/// Sample a divergence-free curl-noise vector at world position `p`,
/// `scale` controls feature size (small → fine swirls, large → broad gusts).
///
/// Computed as `∇ × ψ(p)` where `ψ` is a 3D vector noise potential.
/// Result is approximately incompressible — ideal for fire/smoke/dust
/// because particles never collapse to a point.
#[must_use]
pub fn curl_noise(p: crate::math::Vec3, scale: f32) -> crate::math::Vec3 {
    let e = 0.01;
    let x = p.x() * scale;
    let y = p.y() * scale;
    let z = p.z() * scale;

    // 3 independent potential components
    let p1 = |x: f32, y: f32, z: f32| vnoise3(x, y, z);
    let p2 = |x: f32, y: f32, z: f32| vnoise3(x + 31.7, y + 17.3, z + 7.1);
    let p3 = |x: f32, y: f32, z: f32| vnoise3(x + 5.5, y + 23.9, z + 41.2);

    // ∂p3/∂y - ∂p2/∂z
    let dx = (p3(x, y + e, z) - p3(x, y - e, z)) - (p2(x, y, z + e) - p2(x, y, z - e));
    // ∂p1/∂z - ∂p3/∂x
    let dy = (p1(x, y, z + e) - p1(x, y, z - e)) - (p3(x + e, y, z) - p3(x - e, y, z));
    // ∂p2/∂x - ∂p1/∂y
    let dz = (p2(x + e, y, z) - p2(x - e, y, z)) - (p1(x, y + e, z) - p1(x, y - e, z));

    let two_e = 2.0 * e;
    crate::math::Vec3::new(dx / two_e, dy / two_e, dz / two_e)
}

/// Apply a curl-noise force to every alive particle in the emitter, scaled
/// by `strength` (units / sec²).
pub fn apply_curl_noise(emitter: &mut ParticleEmitter, scale: f32, strength: f32, dt: f32) {
    for p in &mut emitter.particles {
        if !p.alive {
            continue;
        }
        let force = curl_noise(p.position, scale);
        let dv = crate::math::Vec3::new(
            force.x() * strength * dt,
            force.y() * strength * dt,
            force.z() * strength * dt,
        );
        p.velocity = p.velocity + dv;
    }
}

// ---------------------------------------------------------------------------
// Trail emitter — chained sub-particles that lag behind a primary emitter
// ---------------------------------------------------------------------------

/// A single trail-particle: one segment of a moving ribbon.
#[derive(Debug, Clone, Copy)]
pub struct TrailParticle {
    pub position: crate::math::Vec3,
    pub life_remaining: f32,
    pub life_initial: f32,
}

/// Emits one [`TrailParticle`] per frame interval at the configured
/// `source` position. Useful for rocket/comet trails, bullet ribbons,
/// dust streams behind moving objects.
#[derive(Debug, Clone)]
pub struct TrailEmitter {
    pub source: crate::math::Vec3,
    pub life_per_segment: f32,
    pub spawn_interval: f32,
    pub trail: Vec<TrailParticle>,
    pub max_trail_len: usize,
    accumulator: f32,
}

impl TrailEmitter {
    #[must_use]
    pub const fn new(source: crate::math::Vec3) -> Self {
        Self {
            source,
            life_per_segment: 1.0,
            spawn_interval: 0.05,
            trail: Vec::new(),
            max_trail_len: 64,
            accumulator: 0.0,
        }
    }

    /// Advance: age existing segments, drop dead ones, and spawn new ones
    /// from `source` at the configured interval.
    pub fn update(&mut self, dt: f32) {
        // Age & cull.
        for p in &mut self.trail {
            p.life_remaining -= dt;
        }
        self.trail.retain(|p| p.life_remaining > 0.0);

        // Spawn at intervals.
        self.accumulator += dt;
        while self.accumulator >= self.spawn_interval {
            self.accumulator -= self.spawn_interval;
            if self.trail.len() >= self.max_trail_len {
                self.trail.remove(0);
            }
            self.trail.push(TrailParticle {
                position: self.source,
                life_remaining: self.life_per_segment,
                life_initial: self.life_per_segment,
            });
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.trail.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.trail.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_dead() {
        let p = Particle::DEAD;
        assert!(!p.alive);
        assert_eq!(p.life_ratio(), 1.0);
    }

    #[test]
    fn particle_life_ratio() {
        let p = Particle {
            lifetime: 0.5,
            max_lifetime: 1.0,
            alive: true,
            ..Particle::DEAD
        };
        assert!((p.life_ratio() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn emitter_config_default() {
        let cfg = EmitterConfig::default();
        assert_eq!(cfg.max_particles, 1000);
        assert_eq!(cfg.emit_rate, 50.0);
    }

    #[test]
    fn emitter_starts_empty() {
        let emitter = ParticleEmitter::new(EmitterConfig::default());
        assert_eq!(emitter.alive_count(), 0);
    }

    #[test]
    fn emitter_emits_particles() {
        let mut config = EmitterConfig::default();
        config.emit_rate = 100.0;
        let mut emitter = ParticleEmitter::new(config);
        emitter.update(1.0);
        assert!(emitter.alive_count() > 0);
    }

    #[test]
    fn emitter_particles_die() {
        let mut config = EmitterConfig::default();
        config.emit_rate = 100.0;
        config.lifetime_min = 0.1;
        config.lifetime_max = 0.1;
        let mut emitter = ParticleEmitter::new(config);
        emitter.update(0.05);
        let alive_mid = emitter.alive_count();
        assert!(alive_mid > 0);
        // After enough time, particles should die
        for _ in 0..20 {
            emitter.update(0.1);
        }
        // With continuous emission and short lifetime, some should be alive
        // but total should be bounded
        assert!(emitter.alive_count() <= emitter.config.max_particles);
    }

    #[test]
    fn emitter_clear() {
        let mut config = EmitterConfig::default();
        config.emit_rate = 100.0;
        let mut emitter = ParticleEmitter::new(config);
        emitter.update(1.0);
        emitter.clear();
        assert_eq!(emitter.alive_count(), 0);
    }

    #[test]
    fn emitter_respects_max() {
        let mut config = EmitterConfig::default();
        config.max_particles = 10;
        config.emit_rate = 1000.0;
        config.lifetime_min = 10.0;
        config.lifetime_max = 10.0;
        let mut emitter = ParticleEmitter::new(config);
        emitter.update(1.0);
        assert!(emitter.alive_count() <= 10);
    }

    #[test]
    fn emitter_gravity() {
        let mut config = EmitterConfig::default();
        config.emit_rate = 10.0;
        config.gravity = Vec3::new(0.0, -10.0, 0.0);
        config.speed_min = 0.0;
        config.speed_max = 0.01;
        let mut emitter = ParticleEmitter::new(config);
        // First update emits particles
        emitter.update(0.5);
        // Second update applies gravity to the emitted particles
        emitter.update(0.5);
        let has_fallen = emitter
            .particles
            .iter()
            .any(|p| p.alive && p.position.y() < 0.0);
        assert!(has_fallen);
    }

    #[test]
    fn emitter_shape_variants() {
        let _ = EmitterShape::Point;
        let _ = EmitterShape::Sphere { radius: 5.0 };
        let _ = EmitterShape::Box {
            half_extents: Vec3::ONE,
        };
        let _ = EmitterShape::Cone {
            radius: 1.0,
            angle: 0.5,
        };
    }

    #[test]
    fn particle_color_interpolation() {
        let mut config = EmitterConfig::default();
        config.emit_rate = 1.0;
        config.color_start = Color::WHITE;
        config.color_end = Color::BLACK;
        config.lifetime_min = 1.0;
        config.lifetime_max = 1.0;
        let mut emitter = ParticleEmitter::new(config);
        emitter.update(1.1);
        emitter.update(0.5);
        for p in &emitter.particles {
            if p.alive {
                assert!(p.color.r <= 1.0);
            }
        }
    }

    #[test]
    fn emitter_sphere_shape() {
        let mut config = EmitterConfig::default();
        config.shape = EmitterShape::Sphere { radius: 2.0 };
        config.emit_rate = 50.0;
        let mut emitter = ParticleEmitter::new(config);
        emitter.update(0.5);
        assert!(emitter.alive_count() > 0);
    }

    #[test]
    fn emitter_box_shape() {
        let mut config = EmitterConfig::default();
        config.shape = EmitterShape::Box {
            half_extents: Vec3::ONE,
        };
        config.emit_rate = 50.0;
        let mut emitter = ParticleEmitter::new(config);
        emitter.update(0.5);
        assert!(emitter.alive_count() > 0);
    }

    #[test]
    fn emitter_position_matters() {
        let mut config = EmitterConfig::default();
        config.emit_rate = 10.0;
        config.speed_min = 0.0;
        config.speed_max = 0.001;
        let mut emitter = ParticleEmitter::new(config);
        emitter.position = Vec3::new(100.0, 200.0, 300.0);
        emitter.update(1.0);
        for p in &emitter.particles {
            if p.alive {
                assert!((p.position.x() - 100.0).abs() < 1.0);
            }
        }
    }

    #[test]
    fn particle_size_interpolation() {
        let mut config = EmitterConfig::default();
        config.emit_rate = 1.0;
        config.size_start = 10.0;
        config.size_end = 0.0;
        config.lifetime_min = 1.0;
        config.lifetime_max = 1.0;
        let mut emitter = ParticleEmitter::new(config);
        emitter.update(1.1);
        emitter.update(0.5);
        for p in &emitter.particles {
            if p.alive {
                assert!(p.size < 10.0);
                assert!(p.size >= 0.0);
            }
        }
    }

    #[test]
    fn many_particles_performance() {
        let mut config = EmitterConfig::default();
        config.max_particles = 10_000;
        config.emit_rate = 5000.0;
        let mut emitter = ParticleEmitter::new(config);
        emitter.update(1.0);
        emitter.update(1.0);
        assert!(emitter.alive_count() > 0);
    }

    // -----------------------------------------------------------------------
    // Curl noise + Trail tests
    // -----------------------------------------------------------------------

    #[test]
    fn curl_noise_is_bounded() {
        // Sample many points, all should produce finite vectors.
        let mut max_mag = 0.0_f32;
        for i in 0..100 {
            let t = i as f32 * 0.1;
            let v = curl_noise(crate::math::Vec3::new(t, t * 0.5, t * 0.3), 0.7);
            assert!(v.x().is_finite() && v.y().is_finite() && v.z().is_finite());
            let mag = (v.x() * v.x() + v.y() * v.y() + v.z() * v.z()).sqrt();
            if mag > max_mag {
                max_mag = mag;
            }
        }
        // Bound is loose because vnoise3 spans roughly [-1,1] and we take
        // central differences over 0.01 — magnitudes around 100 are
        // typical, far less than 1000.
        assert!(max_mag < 1000.0, "curl noise magnitudes blew up: {max_mag}");
    }

    #[test]
    fn curl_noise_deterministic() {
        let p = crate::math::Vec3::new(1.0, 2.0, 3.0);
        let a = curl_noise(p, 0.5);
        let b = curl_noise(p, 0.5);
        assert!((a.x() - b.x()).abs() < f32::EPSILON);
        assert!((a.y() - b.y()).abs() < f32::EPSILON);
        assert!((a.z() - b.z()).abs() < f32::EPSILON);
    }

    #[test]
    fn apply_curl_noise_changes_velocity() {
        let cfg = EmitterConfig {
            max_particles: 10,
            emit_rate: 5.0,
            lifetime_min: 5.0,
            lifetime_max: 5.0,
            speed_min: 1.0,
            speed_max: 1.0,
            size_start: 1.0,
            size_end: 1.0,
            color_start: Color::WHITE,
            color_end: Color::WHITE,
            gravity: crate::math::Vec3::ZERO,
            shape: EmitterShape::Point,
            world_space: true,
        };
        let mut emitter = ParticleEmitter::new(cfg);
        emitter.update(1.0); // spawn some particles
        let before: Vec<_> = emitter.particles.iter().map(|p| p.velocity).collect();
        apply_curl_noise(&mut emitter, 0.5, 10.0, 0.1);
        let after: Vec<_> = emitter.particles.iter().map(|p| p.velocity).collect();
        // At least one alive particle should have a changed velocity.
        let any_changed = before.iter().zip(after.iter()).any(|(b, a)| {
            (a.x() - b.x()).abs() > 1e-6
                || (a.y() - b.y()).abs() > 1e-6
                || (a.z() - b.z()).abs() > 1e-6
        });
        assert!(any_changed);
    }

    #[test]
    fn trail_emitter_spawns_at_interval() {
        let mut t = TrailEmitter::new(crate::math::Vec3::ZERO);
        t.spawn_interval = 0.1;
        t.life_per_segment = 1.0;
        // 1 second @ 0.1s interval → 10 segments.
        for _ in 0..10 {
            t.update(0.1);
        }
        assert!((t.len() as i32 - 10).abs() <= 1);
    }

    #[test]
    fn trail_emitter_ages_and_drops() {
        let mut t = TrailEmitter::new(crate::math::Vec3::ZERO);
        t.spawn_interval = 0.05;
        t.life_per_segment = 0.2;
        // Spawn segments for 0.5s
        for _ in 0..10 {
            t.update(0.05);
        }
        let count_after_spawn = t.len();
        // Stop spawning further by raising spawn_interval far above dt,
        // then advance time so all current segments expire.
        t.spawn_interval = 999.0;
        t.update(1.0); // > life_per_segment → everyone dies
        assert!(t.len() < count_after_spawn);
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn trail_respects_max_len() {
        let mut t = TrailEmitter::new(crate::math::Vec3::ZERO);
        t.spawn_interval = 0.01;
        t.life_per_segment = 100.0; // never die during test
        t.max_trail_len = 8;
        for _ in 0..200 {
            t.update(0.01);
        }
        assert!(t.len() <= 8);
    }
}
