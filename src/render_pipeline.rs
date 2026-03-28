//! Render pipeline: frame lifecycle, clear, present, uniform buffers.
//!
//! Abstracts the per-frame GPU operations without requiring an actual
//! GPU device (testable CPU-side).

use crate::math::{Color, Mat4, Vec3};
use crate::scene_graph::{NodeKind, Projection, SceneGraph};

// ---------------------------------------------------------------------------
// UniformData
// ---------------------------------------------------------------------------

/// MVP uniform data for the `GBuffer` pass.
#[derive(Debug, Clone, Copy)]
pub struct MvpUniforms {
    pub model: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub projection: [[f32; 4]; 4],
}

impl MvpUniforms {
    #[must_use]
    pub const fn new(model: Mat4, view: Mat4, projection: Mat4) -> Self {
        Self {
            model: model.0.to_cols_array_2d(),
            view: view.0.to_cols_array_2d(),
            projection: projection.0.to_cols_array_2d(),
        }
    }

    #[must_use]
    pub const fn identity() -> Self {
        Self::new(Mat4::IDENTITY, Mat4::IDENTITY, Mat4::IDENTITY)
    }
}

/// PBR material uniform data.
#[derive(Debug, Clone, Copy)]
pub struct MaterialUniforms {
    pub albedo: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emission_strength: f32,
    pub pad: f32,
}

impl MaterialUniforms {
    #[must_use]
    pub const fn from_color(color: Color, metallic: f32, roughness: f32) -> Self {
        Self {
            albedo: color.to_array(),
            metallic,
            roughness,
            emission_strength: 0.0,
            pad: 0.0,
        }
    }
}

impl Default for MaterialUniforms {
    fn default() -> Self {
        Self::from_color(Color::WHITE, 0.0, 0.5)
    }
}

// ---------------------------------------------------------------------------
// FrameData — collected per frame
// ---------------------------------------------------------------------------

/// Per-frame rendering data extracted from the scene.
#[derive(Debug, Clone)]
pub struct FrameData {
    pub camera_view: Mat4,
    pub camera_projection: Mat4,
    pub camera_position: Vec3,
    pub clear_color: Color,
    pub mesh_draws: Vec<MeshDraw>,
    pub sdf_draws: Vec<SdfDraw>,
    pub light_count: u32,
}

/// A mesh to draw this frame.
#[derive(Debug, Clone)]
pub struct MeshDraw {
    pub model_matrix: Mat4,
    pub mesh_id: u32,
    pub material_id: u32,
}

/// An SDF volume to raymarch this frame.
#[derive(Debug, Clone)]
pub struct SdfDraw {
    pub world_position: Vec3,
    pub half_extents: Vec3,
    pub sdf_json: String,
}

impl FrameData {
    /// Extracts frame data from a scene graph.
    #[must_use]
    pub fn from_scene(scene: &SceneGraph) -> Option<Self> {
        let cameras = scene.cameras();
        let camera_id = cameras.first()?;
        let cam_node = scene.get(*camera_id)?;

        let NodeKind::Camera(ref cam_data) = cam_node.kind else {
            return None;
        };

        let cam_world = scene.world_matrix(*camera_id);
        let camera_position = cam_world.transform_point3(Vec3::ZERO);
        let camera_view = cam_world.inverse();
        let camera_projection = match cam_data.projection {
            Projection::Perspective { fov_y, near, far } => {
                Mat4::perspective(fov_y, 16.0 / 9.0, near, far)
            }
            Projection::Orthographic {
                width,
                height,
                near,
                far,
            } => Mat4::orthographic(width, height, near, far),
        };

        let mut mesh_draws = Vec::new();
        for mesh_id in scene.meshes() {
            if let Some(node) = scene.get(mesh_id) {
                if let NodeKind::Mesh(ref md) = node.kind {
                    mesh_draws.push(MeshDraw {
                        model_matrix: scene.world_matrix(mesh_id),
                        mesh_id: md.mesh_id,
                        material_id: md.material_id,
                    });
                }
            }
        }

        let mut sdf_draws = Vec::new();
        for sdf_id in scene.sdf_volumes() {
            if let Some(node) = scene.get(sdf_id) {
                if let NodeKind::Sdf(ref sd) = node.kind {
                    let world = scene.world_matrix(sdf_id);
                    sdf_draws.push(SdfDraw {
                        world_position: world.transform_point3(Vec3::ZERO),
                        half_extents: sd.half_extents,
                        sdf_json: sd.sdf_json.clone(),
                    });
                }
            }
        }

        Some(Self {
            camera_view,
            camera_projection,
            camera_position,
            clear_color: cam_data.clear_color,
            mesh_draws,
            sdf_draws,
            light_count: scene.lights().len() as u32,
        })
    }

    #[must_use]
    pub const fn total_draw_count(&self) -> usize {
        self.mesh_draws.len() + self.sdf_draws.len()
    }
}

// ---------------------------------------------------------------------------
// RenderStats
// ---------------------------------------------------------------------------

/// Statistics for a rendered frame.
#[derive(Debug, Clone, Default)]
pub struct RenderStats {
    pub mesh_draw_calls: u32,
    pub sdf_draw_calls: u32,
    pub triangles_submitted: u64,
    pub uniform_uploads: u32,
    pub texture_binds: u32,
}

impl RenderStats {
    #[must_use]
    pub const fn total_draw_calls(&self) -> u32 {
        self.mesh_draw_calls + self.sdf_draw_calls
    }
}

// ---------------------------------------------------------------------------
// PipelineState
// ---------------------------------------------------------------------------

/// Tracks which pipeline stages are active.
#[derive(Debug, Clone)]
pub struct PipelineState {
    pub gbuffer_enabled: bool,
    pub sdf_raymarch_enabled: bool,
    pub deferred_lighting_enabled: bool,
    pub post_process_enabled: bool,
    pub debug_overlay_enabled: bool,
}

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            gbuffer_enabled: true,
            sdf_raymarch_enabled: true,
            deferred_lighting_enabled: true,
            post_process_enabled: true,
            debug_overlay_enabled: false,
        }
    }
}

impl PipelineState {
    /// Returns the number of enabled stages.
    #[must_use]
    pub fn enabled_stage_count(&self) -> u32 {
        u32::from(self.gbuffer_enabled)
            + u32::from(self.sdf_raymarch_enabled)
            + u32::from(self.deferred_lighting_enabled)
            + u32::from(self.post_process_enabled)
            + u32::from(self.debug_overlay_enabled)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_graph::*;

    #[test]
    fn mvp_uniforms_identity() {
        let u = MvpUniforms::identity();
        assert_eq!(u.model[0][0], 1.0);
        assert_eq!(u.view[1][1], 1.0);
    }

    #[test]
    fn mvp_uniforms_from_matrices() {
        let m = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let u = MvpUniforms::new(m, Mat4::IDENTITY, Mat4::IDENTITY);
        assert!((u.model[3][0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn material_uniforms_default() {
        let m = MaterialUniforms::default();
        assert_eq!(m.metallic, 0.0);
        assert_eq!(m.roughness, 0.5);
    }

    #[test]
    fn material_uniforms_from_color() {
        let m = MaterialUniforms::from_color(Color::RED, 1.0, 0.1);
        assert_eq!(m.albedo[0], 1.0);
        assert_eq!(m.metallic, 1.0);
    }

    #[test]
    fn frame_data_from_empty_scene() {
        let sg = SceneGraph::new("empty");
        assert!(FrameData::from_scene(&sg).is_none());
    }

    #[test]
    fn frame_data_from_scene_with_camera() {
        let mut sg = SceneGraph::new("test");
        sg.add(Node::new("cam", NodeKind::Camera(CameraData::default())));
        sg.update_world_matrices();
        let fd = FrameData::from_scene(&sg).unwrap();
        assert_eq!(fd.mesh_draws.len(), 0);
        assert_eq!(fd.sdf_draws.len(), 0);
    }

    #[test]
    fn frame_data_with_meshes_and_sdf() {
        let mut sg = SceneGraph::new("test");
        sg.add(Node::new("cam", NodeKind::Camera(CameraData::default())));
        sg.add(Node::new("mesh1", NodeKind::Mesh(MeshData::default())));
        sg.add(Node::new("mesh2", NodeKind::Mesh(MeshData::default())));
        sg.add(Node::new("sdf1", NodeKind::Sdf(SdfData::default())));
        sg.add(Node::new("light1", NodeKind::Light(LightData::default())));
        sg.update_world_matrices();
        let fd = FrameData::from_scene(&sg).unwrap();
        assert_eq!(fd.mesh_draws.len(), 2);
        assert_eq!(fd.sdf_draws.len(), 1);
        assert_eq!(fd.light_count, 1);
        assert_eq!(fd.total_draw_count(), 3);
    }

    #[test]
    fn render_stats_total() {
        let stats = RenderStats {
            mesh_draw_calls: 10,
            sdf_draw_calls: 3,
            ..RenderStats::default()
        };
        assert_eq!(stats.total_draw_calls(), 13);
    }

    #[test]
    fn pipeline_state_default() {
        let ps = PipelineState::default();
        assert!(ps.gbuffer_enabled);
        assert!(ps.sdf_raymarch_enabled);
        assert!(!ps.debug_overlay_enabled);
        assert_eq!(ps.enabled_stage_count(), 4);
    }

    #[test]
    fn pipeline_state_all_disabled() {
        let ps = PipelineState {
            gbuffer_enabled: false,
            sdf_raymarch_enabled: false,
            deferred_lighting_enabled: false,
            post_process_enabled: false,
            debug_overlay_enabled: false,
        };
        assert_eq!(ps.enabled_stage_count(), 0);
    }

    #[test]
    fn mesh_draw_has_transform() {
        let draw = MeshDraw {
            model_matrix: Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0)),
            mesh_id: 0,
            material_id: 0,
        };
        let p = draw.model_matrix.transform_point3(Vec3::ZERO);
        assert!((p.x() - 5.0).abs() < 1e-6);
    }

    #[test]
    fn sdf_draw_position() {
        let draw = SdfDraw {
            world_position: Vec3::new(1.0, 2.0, 3.0),
            half_extents: Vec3::ONE,
            sdf_json: String::new(),
        };
        assert_eq!(draw.world_position.y(), 2.0);
    }

    #[test]
    fn render_stats_default() {
        let s = RenderStats::default();
        assert_eq!(s.total_draw_calls(), 0);
    }
}
