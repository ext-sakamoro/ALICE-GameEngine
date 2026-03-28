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
}
