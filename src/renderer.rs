//! Deferred rendering pipeline via wgpu (Vulkan/Metal/DX12/WebGPU).
//!
//! Architecture inspired by Fyrox's 5-attachment `GBuffer`, but targeting wgpu
//! instead of OpenGL for wider platform support and modern API access.
//!
//! ## `GBuffer` layout
//!
//! | RT | Format | Contents |
//! |----|--------|----------|
//! | 0 | RGBA8 Unorm sRGB | Albedo RGB + alpha |
//! | 1 | RGB10A2 Unorm | World-space normal |
//! | 2 | RGBA8 Unorm | Emission RGB + AO |
//! | 3 | RGBA8 Unorm | Metallic / Roughness / flags / spare |
//! | Depth | Depth32Float | Depth buffer |

use crate::math::{Color, Mat4, Vec3};
use crate::scene_graph::{LightVariant, NodeKind, Projection, SceneGraph};

// ---------------------------------------------------------------------------
// Render settings
// ---------------------------------------------------------------------------

/// Quality settings for the renderer.
#[derive(Debug, Clone, PartialEq)]
pub struct QualitySettings {
    pub shadow_map_size: u32,
    pub shadow_cascades: u32,
    pub ssao_enabled: bool,
    pub ssao_samples: u32,
    pub bloom_enabled: bool,
    pub bloom_threshold: f32,
    pub fxaa_enabled: bool,
    pub hdr_enabled: bool,
    pub hdr_exposure: f32,
}

impl Default for QualitySettings {
    fn default() -> Self {
        Self {
            shadow_map_size: 2048,
            shadow_cascades: 4,
            ssao_enabled: true,
            ssao_samples: 16,
            bloom_enabled: true,
            bloom_threshold: 1.0,
            fxaa_enabled: true,
            hdr_enabled: true,
            hdr_exposure: 1.0,
        }
    }
}

// ---------------------------------------------------------------------------
// GBuffer descriptor
// ---------------------------------------------------------------------------

/// Describes the deferred `GBuffer` layout.
#[derive(Debug, Clone)]
pub struct GBufferDesc {
    pub width: u32,
    pub height: u32,
}

impl GBufferDesc {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub const fn pixel_count(&self) -> u32 {
        self.width * self.height
    }
}

// ---------------------------------------------------------------------------
// Draw statistics
// ---------------------------------------------------------------------------

/// Per-frame rendering statistics.
#[derive(Debug, Clone, Default)]
pub struct DrawStats {
    pub draw_calls: u32,
    pub triangles: u64,
    pub mesh_nodes: u32,
    pub sdf_nodes: u32,
    pub light_count: u32,
    pub shadow_passes: u32,
}

// ---------------------------------------------------------------------------
// RenderPass
// ---------------------------------------------------------------------------

/// Named render pass for pipeline management.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RenderPass {
    GBuffer,
    DirectionalShadow,
    PointShadow,
    SpotShadow,
    DeferredLighting,
    SdfRaymarch,
    Ssao,
    Bloom,
    Hdr,
    Fxaa,
    Ui,
    Debug,
    Custom(String),
}

// ---------------------------------------------------------------------------
// MaterialProperty
// ---------------------------------------------------------------------------

/// PBR material properties.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialProperties {
    pub albedo: Color,
    pub metallic: f32,
    pub roughness: f32,
    pub emission: Color,
    pub emission_strength: f32,
    pub normal_strength: f32,
    pub ao_strength: f32,
    pub albedo_texture: Option<u32>,
    pub normal_texture: Option<u32>,
    pub metallic_roughness_texture: Option<u32>,
}

impl Default for MaterialProperties {
    fn default() -> Self {
        Self {
            albedo: Color::WHITE,
            metallic: 0.0,
            roughness: 0.5,
            emission: Color::BLACK,
            emission_strength: 0.0,
            normal_strength: 1.0,
            ao_strength: 1.0,
            albedo_texture: None,
            normal_texture: None,
            metallic_roughness_texture: None,
        }
    }
}

// ---------------------------------------------------------------------------
// ShadowCascade
// ---------------------------------------------------------------------------

/// Parameters for a single CSM cascade.
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowCascade {
    pub near: f32,
    pub far: f32,
    pub view_proj: Mat4,
}

impl ShadowCascade {
    #[must_use]
    pub const fn new(near: f32, far: f32) -> Self {
        Self {
            near,
            far,
            view_proj: Mat4::IDENTITY,
        }
    }
}

// ---------------------------------------------------------------------------
// FrameContext
// ---------------------------------------------------------------------------

/// Per-frame data collected from the scene graph for rendering.
#[derive(Debug, Clone)]
pub struct FrameContext {
    pub view: Mat4,
    pub projection: Mat4,
    pub camera_position: Vec3,
    pub clear_color: Color,
    pub lights: Vec<LightRenderData>,
    pub quality: QualitySettings,
}

/// Light data ready for the shader.
#[derive(Debug, Clone)]
pub struct LightRenderData {
    pub position: Vec3,
    pub direction: Vec3,
    pub color: Color,
    pub intensity: f32,
    pub variant: LightVariant,
    pub cast_shadows: bool,
}

impl FrameContext {
    /// Extracts render data from a scene graph for a given camera.
    #[must_use]
    pub fn from_scene(
        scene: &SceneGraph,
        camera_node_id: crate::scene_graph::NodeId,
    ) -> Option<Self> {
        let cam_node = scene.get(camera_node_id)?;
        let NodeKind::Camera(camera_data) = &cam_node.kind else {
            return None;
        };

        let world = scene.world_matrix(camera_node_id);
        let camera_position = world.transform_point3(Vec3::ZERO);
        let view = world.inverse();

        let projection = match camera_data.projection {
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

        let mut lights = Vec::new();
        for light_id in scene.lights() {
            if let Some(light_node) = scene.get(light_id) {
                if let NodeKind::Light(ld) = &light_node.kind {
                    let lw = scene.world_matrix(light_id);
                    lights.push(LightRenderData {
                        position: lw.transform_point3(Vec3::ZERO),
                        direction: lw.transform_vector3(Vec3::new(0.0, 0.0, -1.0)),
                        color: ld.color,
                        intensity: ld.intensity,
                        variant: ld.variant,
                        cast_shadows: ld.cast_shadows,
                    });
                }
            }
        }

        Some(Self {
            view,
            projection,
            camera_position,
            clear_color: camera_data.clear_color,
            lights,
            quality: QualitySettings::default(),
        })
    }
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

/// The deferred renderer. Manages quality settings, `GBuffer` dimensions,
/// and per-frame draw statistics. GPU pipeline creation is handled by
/// `GpuContext` in the `gpu` module when a window surface is available.
pub struct Renderer {
    pub quality: QualitySettings,
    pub gbuffer: GBufferDesc,
    pub stats: DrawStats,
}

impl Renderer {
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            quality: QualitySettings::default(),
            gbuffer: GBufferDesc::new(width, height),
            stats: DrawStats::default(),
        }
    }

    /// Resize the `GBuffer`.
    pub const fn resize(&mut self, width: u32, height: u32) {
        self.gbuffer = GBufferDesc::new(width, height);
    }

    /// Executes a frame render pass against a scene graph. Collects draw
    /// statistics. GPU command encoding is driven by `GpuContext`.
    #[must_use]
    pub fn render_frame_with_scene(
        &mut self,
        ctx: &FrameContext,
        scene: &crate::scene_graph::SceneGraph,
    ) -> DrawStats {
        let mesh_count = scene.meshes().len() as u32;
        let sdf_count = scene.sdf_volumes().len() as u32;
        let stats = DrawStats {
            draw_calls: mesh_count + sdf_count,
            triangles: u64::from(mesh_count) * 12, // conservative estimate per mesh
            mesh_nodes: mesh_count,
            sdf_nodes: sdf_count,
            light_count: ctx.lights.len() as u32,
            shadow_passes: ctx.lights.iter().filter(|l| l.cast_shadows).count() as u32,
        };
        self.stats = stats.clone();
        stats
    }

    /// Executes a frame render pass from a `FrameContext` only (no scene access).
    #[must_use]
    pub fn render_frame(&mut self, ctx: &FrameContext) -> DrawStats {
        let stats = DrawStats {
            draw_calls: 0,
            triangles: 0,
            mesh_nodes: 0,
            sdf_nodes: 0,
            light_count: ctx.lights.len() as u32,
            shadow_passes: ctx.lights.iter().filter(|l| l.cast_shadows).count() as u32,
        };
        self.stats = stats.clone();
        stats
    }

    /// Returns passes that would execute for current quality settings.
    #[must_use]
    pub fn active_passes(&self) -> Vec<RenderPass> {
        let mut passes = vec![RenderPass::GBuffer, RenderPass::DeferredLighting];
        if self.quality.ssao_enabled {
            passes.push(RenderPass::Ssao);
        }
        if self.quality.bloom_enabled {
            passes.push(RenderPass::Bloom);
        }
        if self.quality.hdr_enabled {
            passes.push(RenderPass::Hdr);
        }
        if self.quality.fxaa_enabled {
            passes.push(RenderPass::Fxaa);
        }
        passes
    }
}

// ---------------------------------------------------------------------------
// RenderGraph — DAG of render passes
// ---------------------------------------------------------------------------

/// A node in the render graph.
#[derive(Debug, Clone)]
pub struct RenderGraphNode {
    pub pass: RenderPass,
    pub dependencies: Vec<usize>,
    pub enabled: bool,
}

/// Render graph: DAG of passes with dependency ordering.
#[derive(Debug, Clone)]
pub struct RenderGraph {
    pub nodes: Vec<RenderGraphNode>,
}

impl RenderGraph {
    #[must_use]
    pub const fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Adds a pass node and returns its index.
    pub fn add_pass(&mut self, pass: RenderPass, dependencies: Vec<usize>) -> usize {
        self.nodes.push(RenderGraphNode {
            pass,
            dependencies,
            enabled: true,
        });
        self.nodes.len() - 1
    }

    /// Returns execution order via Kahn's topological sort.
    #[must_use]
    pub fn execution_order(&self) -> Vec<usize> {
        let n = self.nodes.len();
        // Count incoming edges for each enabled node.
        let mut in_degree = vec![0u32; n];
        for (i, node) in self.nodes.iter().enumerate() {
            if !node.enabled {
                continue;
            }
            for &dep in &node.dependencies {
                if dep < n && self.nodes[dep].enabled {
                    in_degree[i] += 1;
                }
            }
        }

        // Seed queue with zero-in-degree nodes.
        let mut queue: Vec<usize> = (0..n)
            .filter(|&i| self.nodes[i].enabled && in_degree[i] == 0)
            .collect();
        let mut order = Vec::new();

        while let Some(idx) = queue.pop() {
            order.push(idx);
            // For every node that depends on `idx`, decrement its in-degree.
            for (j, node) in self.nodes.iter().enumerate() {
                if !node.enabled || j == idx {
                    continue;
                }
                if node.dependencies.contains(&idx) {
                    in_degree[j] -= 1;
                    if in_degree[j] == 0 {
                        queue.push(j);
                    }
                }
            }
        }
        order
    }

    #[must_use]
    pub const fn pass_count(&self) -> usize {
        self.nodes.len()
    }
}

impl Default for RenderGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// DebugRenderer — wireframe lines, AABBs, etc.
// ---------------------------------------------------------------------------

/// A debug line segment for visualization.
#[derive(Debug, Clone, Copy)]
pub struct DebugLine {
    pub start: Vec3,
    pub end: Vec3,
    pub color: Color,
}

/// Accumulates debug draw commands for one frame.
#[derive(Debug, Clone)]
pub struct DebugRenderer {
    pub lines: Vec<DebugLine>,
    pub enabled: bool,
}

impl DebugRenderer {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            lines: Vec::new(),
            enabled: true,
        }
    }

    /// Draws a line.
    pub fn line(&mut self, start: Vec3, end: Vec3, color: Color) {
        if self.enabled {
            self.lines.push(DebugLine { start, end, color });
        }
    }

    /// Draws a wireframe AABB.
    pub fn aabb(&mut self, min: Vec3, max: Vec3, color: Color) {
        if !self.enabled {
            return;
        }
        let corners = [
            Vec3::new(min.x(), min.y(), min.z()),
            Vec3::new(max.x(), min.y(), min.z()),
            Vec3::new(max.x(), max.y(), min.z()),
            Vec3::new(min.x(), max.y(), min.z()),
            Vec3::new(min.x(), min.y(), max.z()),
            Vec3::new(max.x(), min.y(), max.z()),
            Vec3::new(max.x(), max.y(), max.z()),
            Vec3::new(min.x(), max.y(), max.z()),
        ];
        let edges = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];
        for &(a, b) in &edges {
            self.line(corners[a], corners[b], color);
        }
    }

    /// Clears all debug draw commands.
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    #[must_use]
    pub const fn line_count(&self) -> usize {
        self.lines.len()
    }
}

impl Default for DebugRenderer {
    fn default() -> Self {
        Self::new()
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
    fn quality_default() {
        let q = QualitySettings::default();
        assert_eq!(q.shadow_map_size, 2048);
        assert!(q.ssao_enabled);
        assert!(q.bloom_enabled);
    }

    #[test]
    fn gbuffer_desc() {
        let g = GBufferDesc::new(1920, 1080);
        assert_eq!(g.pixel_count(), 1920 * 1080);
    }

    #[test]
    fn draw_stats_default() {
        let s = DrawStats::default();
        assert_eq!(s.draw_calls, 0);
    }

    #[test]
    fn renderer_new() {
        let r = Renderer::new(1920, 1080);
        assert_eq!(r.gbuffer.width, 1920);
    }

    #[test]
    fn renderer_resize() {
        let mut r = Renderer::new(1920, 1080);
        r.resize(3840, 2160);
        assert_eq!(r.gbuffer.width, 3840);
        assert_eq!(r.gbuffer.height, 2160);
    }

    #[test]
    fn active_passes_all() {
        let r = Renderer::new(1920, 1080);
        let passes = r.active_passes();
        assert!(passes.contains(&RenderPass::GBuffer));
        assert!(passes.contains(&RenderPass::DeferredLighting));
        assert!(passes.contains(&RenderPass::Ssao));
        assert!(passes.contains(&RenderPass::Bloom));
        assert!(passes.contains(&RenderPass::Hdr));
        assert!(passes.contains(&RenderPass::Fxaa));
    }

    #[test]
    fn active_passes_minimal() {
        let mut r = Renderer::new(800, 600);
        r.quality.ssao_enabled = false;
        r.quality.bloom_enabled = false;
        r.quality.hdr_enabled = false;
        r.quality.fxaa_enabled = false;
        let passes = r.active_passes();
        assert_eq!(passes.len(), 2);
    }

    #[test]
    fn material_properties_default() {
        let m = MaterialProperties::default();
        assert_eq!(m.metallic, 0.0);
        assert_eq!(m.roughness, 0.5);
    }

    #[test]
    fn shadow_cascade() {
        let c = ShadowCascade::new(0.1, 50.0);
        assert_eq!(c.near, 0.1);
        assert_eq!(c.far, 50.0);
    }

    #[test]
    fn render_pass_custom() {
        let p = RenderPass::Custom("my_pass".to_string());
        assert!(matches!(p, RenderPass::Custom(_)));
    }

    #[test]
    fn frame_context_from_scene() {
        let mut sg = SceneGraph::new("test");
        let cam = sg.add(Node::new("cam", NodeKind::Camera(CameraData::default())));
        sg.add(Node::new("light", NodeKind::Light(LightData::default())));
        sg.update_world_matrices();

        let ctx = FrameContext::from_scene(&sg, cam).unwrap();
        assert_eq!(ctx.lights.len(), 1);
    }

    #[test]
    fn frame_context_returns_none_for_non_camera() {
        let mut sg = SceneGraph::new("test");
        let empty = sg.add(Node::new("not_cam", NodeKind::Empty));
        sg.update_world_matrices();
        assert!(FrameContext::from_scene(&sg, empty).is_none());
    }

    #[test]
    fn render_frame_stats() {
        let mut sg = SceneGraph::new("test");
        let cam = sg.add(Node::new("cam", NodeKind::Camera(CameraData::default())));
        sg.add(Node::new(
            "sun",
            NodeKind::Light(LightData {
                cast_shadows: true,
                ..LightData::default()
            }),
        ));
        sg.add(Node::new(
            "fill",
            NodeKind::Light(LightData {
                cast_shadows: false,
                ..LightData::default()
            }),
        ));
        sg.update_world_matrices();

        let ctx = FrameContext::from_scene(&sg, cam).unwrap();
        let mut renderer = Renderer::new(1920, 1080);
        let stats = renderer.render_frame(&ctx);
        assert_eq!(stats.light_count, 2);
        assert_eq!(stats.shadow_passes, 1);
    }

    #[test]
    fn sdf_render_pass_exists() {
        let p = RenderPass::SdfRaymarch;
        assert!(matches!(p, RenderPass::SdfRaymarch));
    }

    #[test]
    fn render_graph_empty() {
        let rg = RenderGraph::new();
        assert_eq!(rg.pass_count(), 0);
        assert!(rg.execution_order().is_empty());
    }

    #[test]
    fn render_graph_linear() {
        let mut rg = RenderGraph::new();
        let gbuf = rg.add_pass(RenderPass::GBuffer, vec![]);
        let lighting = rg.add_pass(RenderPass::DeferredLighting, vec![gbuf]);
        let fxaa = rg.add_pass(RenderPass::Fxaa, vec![lighting]);
        let order = rg.execution_order();
        assert_eq!(order.len(), 3);
        let gbuf_pos = order.iter().position(|&x| x == gbuf).unwrap();
        let light_pos = order.iter().position(|&x| x == lighting).unwrap();
        let fxaa_pos = order.iter().position(|&x| x == fxaa).unwrap();
        assert!(gbuf_pos < light_pos);
        assert!(light_pos < fxaa_pos);
    }

    #[test]
    fn render_graph_disabled_pass() {
        let mut rg = RenderGraph::new();
        let gbuf = rg.add_pass(RenderPass::GBuffer, vec![]);
        rg.nodes[gbuf].enabled = false;
        let order = rg.execution_order();
        assert!(order.is_empty());
    }

    #[test]
    fn debug_renderer_line() {
        let mut dr = DebugRenderer::new();
        dr.line(Vec3::ZERO, Vec3::ONE, Color::RED);
        assert_eq!(dr.line_count(), 1);
    }

    #[test]
    fn debug_renderer_aabb() {
        let mut dr = DebugRenderer::new();
        dr.aabb(-Vec3::ONE, Vec3::ONE, Color::GREEN);
        assert_eq!(dr.line_count(), 12);
    }

    #[test]
    fn debug_renderer_clear() {
        let mut dr = DebugRenderer::new();
        dr.line(Vec3::ZERO, Vec3::ONE, Color::WHITE);
        dr.clear();
        assert_eq!(dr.line_count(), 0);
    }

    #[test]
    fn debug_renderer_disabled() {
        let mut dr = DebugRenderer::new();
        dr.enabled = false;
        dr.line(Vec3::ZERO, Vec3::ONE, Color::WHITE);
        assert_eq!(dr.line_count(), 0);
    }

    #[test]
    fn render_graph_parallel_passes() {
        let mut rg = RenderGraph::new();
        let gbuf = rg.add_pass(RenderPass::GBuffer, vec![]);
        let _shadow = rg.add_pass(RenderPass::DirectionalShadow, vec![]);
        let _lighting = rg.add_pass(RenderPass::DeferredLighting, vec![gbuf]);
        let order = rg.execution_order();
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn render_frame_with_scene_counts() {
        let mut sg = SceneGraph::new("test");
        let cam = sg.add(Node::new("cam", NodeKind::Camera(CameraData::default())));
        sg.add(Node::new("m1", NodeKind::Mesh(MeshData::default())));
        sg.add(Node::new("m2", NodeKind::Mesh(MeshData::default())));
        sg.add(Node::new("s1", NodeKind::Sdf(SdfData::default())));
        sg.add(Node::new("sun", NodeKind::Light(LightData::default())));
        sg.update_world_matrices();

        let ctx = FrameContext::from_scene(&sg, cam).unwrap();
        let mut r = Renderer::new(1920, 1080);
        let stats = r.render_frame_with_scene(&ctx, &sg);
        assert_eq!(stats.mesh_nodes, 2);
        assert_eq!(stats.sdf_nodes, 1);
        assert_eq!(stats.draw_calls, 3);
        assert_eq!(stats.light_count, 1);
    }

    #[test]
    fn render_graph_topo_correct_order() {
        let mut rg = RenderGraph::new();
        let a = rg.add_pass(RenderPass::GBuffer, vec![]);
        let b = rg.add_pass(RenderPass::Ssao, vec![a]);
        let c = rg.add_pass(RenderPass::DeferredLighting, vec![a, b]);
        let order = rg.execution_order();
        let pos_a = order.iter().position(|&x| x == a).unwrap();
        let pos_b = order.iter().position(|&x| x == b).unwrap();
        let pos_c = order.iter().position(|&x| x == c).unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn orthographic_frame_context() {
        let mut sg = SceneGraph::new("test");
        sg.add(Node::new(
            "cam",
            NodeKind::Camera(CameraData {
                projection: Projection::Orthographic {
                    width: 20.0,
                    height: 15.0,
                    near: 0.1,
                    far: 100.0,
                },
                ..CameraData::default()
            }),
        ));
        sg.update_world_matrices();
        let ctx = FrameContext::from_scene(&sg, NodeId(0)).unwrap();
        assert_ne!(ctx.projection, Mat4::IDENTITY);
    }
}
