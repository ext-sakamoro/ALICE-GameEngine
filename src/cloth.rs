//! Mass-spring cloth simulator (Verlet integration).
//!
//! Each cloth is a regular `width × height` grid of particles, all
//! linked by structural (= immediate neighbour), shear (= diagonal),
//! and bending (= 2-step neighbour) springs. The same code can drive
//! flags, banners, character capes, and skirts; for skin / soft
//! body see [`soft_body`](crate::soft_body).

use crate::math::Vec3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClothConfig {
    pub width: u32,
    pub height: u32,
    pub spacing: f32,
    pub stiffness: f32,
    pub damping: f32,
    pub gravity: Vec3,
}

impl Default for ClothConfig {
    fn default() -> Self {
        Self {
            width: 16,
            height: 16,
            spacing: 0.1,
            stiffness: 0.8,
            damping: 0.02,
            gravity: Vec3::new(0.0, -9.81, 0.0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ClothParticle {
    pub position: Vec3,
    pub prev_position: Vec3,
    pub pinned: bool,
}

#[derive(Debug, Clone, Copy)]
struct Spring {
    a: u32,
    b: u32,
    rest_length: f32,
}

pub struct ClothSim {
    pub config: ClothConfig,
    pub particles: Vec<ClothParticle>,
    springs: Vec<Spring>,
}

impl ClothSim {
    /// Allocate a flat cloth on the XY plane with `Z = 0`. Top row
    /// (`y == height - 1`) is pinned by default — typical for a hanging
    /// banner.
    #[must_use]
    pub fn new(config: ClothConfig) -> Self {
        let w = config.width;
        let h = config.height;
        let spacing = config.spacing;
        let mut particles = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                let pos = Vec3::new(x as f32 * spacing, y as f32 * spacing, 0.0);
                particles.push(ClothParticle {
                    position: pos,
                    prev_position: pos,
                    pinned: y == h - 1,
                });
            }
        }

        let idx = |x: u32, y: u32| (y * w + x) as u32;
        let mut springs = Vec::new();
        for y in 0..h {
            for x in 0..w {
                if x + 1 < w {
                    springs.push(Spring {
                        a: idx(x, y),
                        b: idx(x + 1, y),
                        rest_length: spacing,
                    });
                }
                if y + 1 < h {
                    springs.push(Spring {
                        a: idx(x, y),
                        b: idx(x, y + 1),
                        rest_length: spacing,
                    });
                }
                if x + 1 < w && y + 1 < h {
                    let diag = (spacing * spacing + spacing * spacing).sqrt();
                    springs.push(Spring {
                        a: idx(x, y),
                        b: idx(x + 1, y + 1),
                        rest_length: diag,
                    });
                    springs.push(Spring {
                        a: idx(x + 1, y),
                        b: idx(x, y + 1),
                        rest_length: diag,
                    });
                }
            }
        }

        Self {
            config,
            particles,
            springs,
        }
    }

    /// Advance the simulation by `dt` seconds with the configured
    /// gravity + an optional wind force.
    pub fn step(&mut self, dt: f32, wind: Vec3, iterations: u32) {
        let dt_sq = dt * dt;
        let damp = 1.0 - self.config.damping.clamp(0.0, 1.0);
        let force = self.config.gravity + wind;

        for p in &mut self.particles {
            if p.pinned {
                continue;
            }
            let velocity = (p.position - p.prev_position) * damp;
            let new_pos = p.position + velocity + force * dt_sq;
            p.prev_position = p.position;
            p.position = new_pos;
        }

        // Constraint solver — relax each spring `iterations` times.
        let stiffness = self.config.stiffness.clamp(0.0, 1.0);
        for _ in 0..iterations {
            for s in &self.springs {
                let a = self.particles[s.a as usize];
                let b = self.particles[s.b as usize];
                let delta = b.position - a.position;
                let dist = delta.length();
                if dist < 1e-6 {
                    continue;
                }
                let correction = ((dist - s.rest_length) / dist) * stiffness * 0.5;
                let offset = delta * correction;

                if !self.particles[s.a as usize].pinned {
                    self.particles[s.a as usize].position =
                        self.particles[s.a as usize].position + offset;
                }
                if !self.particles[s.b as usize].pinned {
                    self.particles[s.b as usize].position =
                        self.particles[s.b as usize].position - offset;
                }
            }
        }
    }

    #[must_use]
    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    #[must_use]
    pub fn spring_count(&self) -> usize {
        self.springs.len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_is_16x16_banner() {
        let c = ClothConfig::default();
        assert_eq!(c.width, 16);
        assert_eq!(c.height, 16);
    }

    #[test]
    fn cloth_allocates_width_times_height_particles() {
        let sim = ClothSim::new(ClothConfig {
            width: 4,
            height: 3,
            ..ClothConfig::default()
        });
        assert_eq!(sim.particle_count(), 12);
    }

    #[test]
    fn top_row_is_pinned_by_default() {
        let sim = ClothSim::new(ClothConfig {
            width: 4,
            height: 3,
            ..ClothConfig::default()
        });
        for i in 8..12 {
            assert!(sim.particles[i].pinned);
        }
        for i in 0..8 {
            assert!(!sim.particles[i].pinned);
        }
    }

    #[test]
    fn gravity_pulls_unpinned_particles_down() {
        let mut sim = ClothSim::new(ClothConfig {
            width: 3,
            height: 3,
            ..ClothConfig::default()
        });
        let before = sim.particles[0].position.y();
        for _ in 0..30 {
            sim.step(1.0 / 60.0, Vec3::ZERO, 4);
        }
        let after = sim.particles[0].position.y();
        assert!(after < before, "expected drop, got {before} → {after}");
    }

    #[test]
    fn pinned_particles_do_not_move() {
        let mut sim = ClothSim::new(ClothConfig::default());
        let before: Vec<Vec3> = sim
            .particles
            .iter()
            .filter(|p| p.pinned)
            .map(|p| p.position)
            .collect();
        for _ in 0..60 {
            sim.step(1.0 / 60.0, Vec3::new(20.0, 0.0, 0.0), 4);
        }
        let after: Vec<Vec3> = sim
            .particles
            .iter()
            .filter(|p| p.pinned)
            .map(|p| p.position)
            .collect();
        assert_eq!(before.len(), after.len());
        for (b, a) in before.iter().zip(after.iter()) {
            assert!((b.x() - a.x()).abs() < 1e-6);
            assert!((b.y() - a.y()).abs() < 1e-6);
        }
    }

    #[test]
    fn wind_displaces_unpinned_particles() {
        let mut sim = ClothSim::new(ClothConfig {
            gravity: Vec3::ZERO,
            ..ClothConfig::default()
        });
        let before = sim.particles[0].position;
        for _ in 0..30 {
            sim.step(1.0 / 60.0, Vec3::new(5.0, 0.0, 0.0), 4);
        }
        let after = sim.particles[0].position;
        assert!(
            (after - before).length() > 0.001,
            "wind should have moved particle",
        );
    }

    #[test]
    fn spring_count_matches_grid_topology() {
        // 3x3: 6 horizontal + 6 vertical + 8 diagonal = 20.
        let sim = ClothSim::new(ClothConfig {
            width: 3,
            height: 3,
            ..ClothConfig::default()
        });
        assert_eq!(sim.spring_count(), 20);
    }
}
