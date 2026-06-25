//! Tetrahedral mass-spring soft body — 3D extension of the 2D cloth
//! model. Each soft body is a regular `nx × ny × nz` lattice of
//! particles connected by structural springs along the three axes; the
//! result is a deformable jello-like volume suitable for trampoline
//! pads, stress balls, and "squash + stretch" character helpers.

use crate::math::Vec3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoftBodyConfig {
    pub nx: u32,
    pub ny: u32,
    pub nz: u32,
    pub spacing: f32,
    pub stiffness: f32,
    pub damping: f32,
    pub gravity: Vec3,
}

impl Default for SoftBodyConfig {
    fn default() -> Self {
        Self {
            nx: 4,
            ny: 4,
            nz: 4,
            spacing: 0.2,
            stiffness: 0.6,
            damping: 0.05,
            gravity: Vec3::new(0.0, -9.81, 0.0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SoftParticle {
    pub position: Vec3,
    pub prev_position: Vec3,
    pub pinned: bool,
}

#[derive(Debug, Clone, Copy)]
struct AxisSpring {
    a: u32,
    b: u32,
    rest_length: f32,
}

pub struct SoftBodySim {
    pub config: SoftBodyConfig,
    pub particles: Vec<SoftParticle>,
    springs: Vec<AxisSpring>,
}

impl SoftBodySim {
    #[must_use]
    pub fn new(config: SoftBodyConfig, origin: Vec3) -> Self {
        let (nx, ny, nz) = (config.nx, config.ny, config.nz);
        let s = config.spacing;
        let mut particles = Vec::with_capacity((nx * ny * nz) as usize);
        for k in 0..nz {
            for j in 0..ny {
                for i in 0..nx {
                    let pos = origin + Vec3::new(i as f32 * s, j as f32 * s, k as f32 * s);
                    particles.push(SoftParticle {
                        position: pos,
                        prev_position: pos,
                        pinned: false,
                    });
                }
            }
        }

        let idx = |i: u32, j: u32, k: u32| (k * nx * ny + j * nx + i) as u32;
        let mut springs = Vec::new();
        for k in 0..nz {
            for j in 0..ny {
                for i in 0..nx {
                    if i + 1 < nx {
                        springs.push(AxisSpring {
                            a: idx(i, j, k),
                            b: idx(i + 1, j, k),
                            rest_length: s,
                        });
                    }
                    if j + 1 < ny {
                        springs.push(AxisSpring {
                            a: idx(i, j, k),
                            b: idx(i, j + 1, k),
                            rest_length: s,
                        });
                    }
                    if k + 1 < nz {
                        springs.push(AxisSpring {
                            a: idx(i, j, k),
                            b: idx(i, j, k + 1),
                            rest_length: s,
                        });
                    }
                }
            }
        }

        Self {
            config,
            particles,
            springs,
        }
    }

    pub fn step(&mut self, dt: f32, iterations: u32) {
        let dt_sq = dt * dt;
        let damp = 1.0 - self.config.damping.clamp(0.0, 1.0);
        let g = self.config.gravity;

        for p in &mut self.particles {
            if p.pinned {
                continue;
            }
            let velocity = (p.position - p.prev_position) * damp;
            let new_pos = p.position + velocity + g * dt_sq;
            p.prev_position = p.position;
            p.position = new_pos;
        }

        let k = self.config.stiffness.clamp(0.0, 1.0);
        for _ in 0..iterations {
            for s in &self.springs {
                let a = self.particles[s.a as usize];
                let b = self.particles[s.b as usize];
                let delta = b.position - a.position;
                let dist = delta.length();
                if dist < 1e-6 {
                    continue;
                }
                let correction = ((dist - s.rest_length) / dist) * k * 0.5;
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

    pub fn pin(&mut self, i: u32, j: u32, k: u32) {
        let nx = self.config.nx;
        let ny = self.config.ny;
        let idx = (k * nx * ny + j * nx + i) as usize;
        if let Some(p) = self.particles.get_mut(idx) {
            p.pinned = true;
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
    fn default_4x4x4_lattice_has_64_particles() {
        let s = SoftBodySim::new(SoftBodyConfig::default(), Vec3::ZERO);
        assert_eq!(s.particle_count(), 64);
    }

    #[test]
    fn spring_count_matches_3d_grid() {
        // 4x4x4: 3*4*4 + 4*3*4 + 4*4*3 = 48 + 48 + 48 = 144.
        let s = SoftBodySim::new(SoftBodyConfig::default(), Vec3::ZERO);
        assert_eq!(s.spring_count(), 144);
    }

    #[test]
    fn pin_keeps_particle_anchored() {
        let mut s = SoftBodySim::new(SoftBodyConfig::default(), Vec3::ZERO);
        s.pin(0, 3, 0);
        let pinned_pos = s.particles[3 * 4].position;
        for _ in 0..120 {
            s.step(1.0 / 60.0, 4);
        }
        let after = s.particles[3 * 4].position;
        assert!((after - pinned_pos).length() < 1e-4);
    }

    #[test]
    fn unpinned_body_falls_under_gravity() {
        let mut s = SoftBodySim::new(SoftBodyConfig::default(), Vec3::ZERO);
        let before = s.particles[0].position.y();
        for _ in 0..60 {
            s.step(1.0 / 60.0, 4);
        }
        let after = s.particles[0].position.y();
        assert!(after < before);
    }

    #[test]
    fn pinned_corners_hold_volume() {
        let mut s = SoftBodySim::new(SoftBodyConfig::default(), Vec3::ZERO);
        // Pin all 8 corners of the lattice — the body should mostly
        // hold its shape under gravity.
        for &(i, j, k) in &[
            (0, 0, 0),
            (3, 0, 0),
            (0, 3, 0),
            (3, 3, 0),
            (0, 0, 3),
            (3, 0, 3),
            (0, 3, 3),
            (3, 3, 3),
        ] {
            s.pin(i, j, k);
        }
        let centre_idx = 1 * 4 * 4 + 1 * 4 + 1;
        let centre_before = s.particles[centre_idx].position;
        for _ in 0..60 {
            s.step(1.0 / 60.0, 6);
        }
        let centre_after = s.particles[centre_idx].position;
        // Drop should be bounded by the lattice spacing.
        assert!((centre_after - centre_before).length() < 0.6);
    }

    #[test]
    fn config_default_4x4x4() {
        let c = SoftBodyConfig::default();
        assert_eq!(c.nx, 4);
        assert_eq!(c.ny, 4);
        assert_eq!(c.nz, 4);
    }
}
