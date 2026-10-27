//! Light occlusion of a posed mesh by its own shape: from each point a ray toward the light, in
//! fixed point ([`alice_physics`]'s `TriMesh` with its BVH and Moller-Trumbore test), the point
//! shadowed when the ray meets any triangle of the scene.
//!
//! The scene is every part's triangles (hair, face, body, clothes), so hair shades the skin under
//! it and the chin shades the neck. The ray starts a small distance along the point's normal (the
//! bias), so a point does not shadow itself on the surface it lies on. The answer is 0 (shadowed)
//! or 1 (lit) per point, the same bits on every device: positions are converted to fixed point
//! once and nothing after that is floating point. A renderer takes it as the light's attenuation
//! of the half-Lambert term before the toon ramp ([`crate::vrm_render::ToonTerms`]).

use alice_physics::math::{Fix128, Vec3Fix};
use alice_physics::raycast::Ray;
use alice_physics::trimesh::TriMesh;

/// Why a scene or a query was refused
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OcclusionError {
    /// Indices that are not whole triangles or name no vertex, a coordinate or a direction that is
    /// not finite, a zero light direction, a negative bias, or points and normals of different
    /// lengths
    BadInput(String),
}

impl std::fmt::Display for OcclusionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadInput(e) => write!(f, "bad occlusion input: {e}"),
        }
    }
}

impl std::error::Error for OcclusionError {}

/// The triangles that can shadow, with their BVH
pub struct OcclusionScene {
    mesh: TriMesh,
    bias: Fix128,
    /// How far a ray looks: past the whole scene from any point inside its bounds
    reach: Fix128,
}

fn fx(v: f32) -> Fix128 {
    Fix128::from_f64(f64::from(v))
}

fn fx3(v: [f32; 3]) -> Vec3Fix {
    Vec3Fix::new(fx(v[0]), fx(v[1]), fx(v[2]))
}

fn finite(v: &[f32; 3]) -> bool {
    v.iter().all(|x| x.is_finite())
}

impl OcclusionScene {
    /// The scene of the given meshes (positions, triangle indices); `bias` is how far along its
    /// normal a point's ray starts (in the positions' units, >= 0)
    ///
    /// # Errors
    ///
    /// [`OcclusionError::BadInput`]
    pub fn new(meshes: &[(&[[f32; 3]], &[u32])], bias: f32) -> Result<Self, OcclusionError> {
        if !bias.is_finite() || bias < 0.0 {
            return Err(OcclusionError::BadInput(format!("bias {bias}")));
        }
        let mut verts: Vec<Vec3Fix> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        for (m, (pos, ind)) in meshes.iter().enumerate() {
            if !ind.len().is_multiple_of(3) {
                return Err(OcclusionError::BadInput(format!(
                    "mesh {m}: {} indices is not whole triangles",
                    ind.len()
                )));
            }
            if let Some(i) = ind.iter().find(|&&i| i as usize >= pos.len()) {
                return Err(OcclusionError::BadInput(format!(
                    "mesh {m}: index {i} past {} vertices",
                    pos.len()
                )));
            }
            if !pos.iter().all(finite) {
                return Err(OcclusionError::BadInput(format!(
                    "mesh {m}: a coordinate is not finite"
                )));
            }
            let base = u32::try_from(verts.len())
                .map_err(|_| OcclusionError::BadInput("too many vertices".into()))?;
            verts.extend(pos.iter().map(|&p| fx3(p)));
            idx.extend(ind.iter().map(|&i| base + i));
        }
        let mesh = TriMesh::from_indexed(&verts, &idx);
        let reach = if mesh.triangles.is_empty() {
            Fix128::ONE
        } else {
            (mesh.bounds.max - mesh.bounds.min).length() + Fix128::ONE
        };
        Ok(Self {
            mesh,
            bias: fx(bias),
            reach,
        })
    }

    /// Per point: 0 when the ray from it (started `bias` along its normal) toward the light meets
    /// a triangle of the scene within reach, 1 otherwise. `toward_light` is the direction from a
    /// point to the light (any length but 0)
    ///
    /// # Errors
    ///
    /// [`OcclusionError::BadInput`]
    pub fn attenuation(
        &self,
        points: &[[f32; 3]],
        normals: &[[f32; 3]],
        toward_light: [f32; 3],
    ) -> Result<Vec<f32>, OcclusionError> {
        if points.len() != normals.len() {
            return Err(OcclusionError::BadInput(format!(
                "{} points and {} normals",
                points.len(),
                normals.len()
            )));
        }
        if !finite(&toward_light) || toward_light.iter().all(|&x| x == 0.0) {
            return Err(OcclusionError::BadInput(format!(
                "light direction {toward_light:?}"
            )));
        }
        if !points.iter().chain(normals).all(finite) {
            return Err(OcclusionError::BadInput(
                "a point or a normal that is not finite".into(),
            ));
        }
        let dir = fx3(toward_light);
        // the scene's reach from the farthest point queried
        let far = points
            .iter()
            .map(|&p| (fx3(p) - self.mesh.bounds.min).length())
            .fold(Fix128::ZERO, |a, b| if b > a { b } else { a });
        let max_t = self.reach + far;
        Ok(points
            .iter()
            .zip(normals)
            .map(|(&p, &n)| {
                let n = fx3(n);
                let len = n.length();
                let lift = if len.is_zero() {
                    Vec3Fix::ZERO
                } else {
                    n / len * self.bias
                };
                let ray = Ray::new(fx3(p) + lift, dir);
                if self.mesh.raycast(&ray, max_t).is_some() {
                    0.0
                } else {
                    1.0
                }
            })
            .collect())
    }
}
