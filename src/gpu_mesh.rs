//! GPU mesh upload: vertex/index buffers and draw commands.

use crate::asset::Vertex;
use crate::math::Mat4;

// ---------------------------------------------------------------------------
// GpuMeshDesc — CPU-side mesh descriptor for GPU upload
// ---------------------------------------------------------------------------

/// Describes a mesh ready for GPU upload.
#[derive(Debug, Clone)]
pub struct GpuMeshDesc {
    pub name: String,
    pub vertex_count: u32,
    pub index_count: u32,
    pub vertex_stride: u32,
}

impl GpuMeshDesc {
    #[must_use]
    pub fn from_asset(name: &str, vertices: &[Vertex], indices: &[u32]) -> Self {
        Self {
            name: name.to_string(),
            vertex_count: vertices.len() as u32,
            index_count: indices.len() as u32,
            vertex_stride: std::mem::size_of::<Vertex>() as u32,
        }
    }

    #[must_use]
    pub const fn triangle_count(&self) -> u32 {
        self.index_count / 3
    }

    /// Estimated GPU memory in bytes.
    #[must_use]
    pub const fn estimated_gpu_bytes(&self) -> u64 {
        let vb = self.vertex_count as u64 * self.vertex_stride as u64;
        let ib = self.index_count as u64 * 4; // u32 indices
        vb + ib
    }
}

// ---------------------------------------------------------------------------
// VertexLayout
// ---------------------------------------------------------------------------

/// Describes the vertex attribute layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexAttribute {
    Position3F,
    Normal3F,
    Uv2F,
    Color4F,
    Tangent4F,
}

/// A complete vertex layout.
#[derive(Debug, Clone)]
pub struct VertexLayout {
    pub attributes: Vec<VertexAttribute>,
}

impl VertexLayout {
    /// Standard PBR layout: position + normal + UV.
    #[must_use]
    pub fn standard() -> Self {
        Self {
            attributes: vec![
                VertexAttribute::Position3F,
                VertexAttribute::Normal3F,
                VertexAttribute::Uv2F,
            ],
        }
    }

    /// Returns the stride in bytes.
    #[must_use]
    pub fn stride(&self) -> u32 {
        self.attributes.iter().map(|a| a.byte_size()).sum()
    }

    /// Returns the number of attributes.
    #[must_use]
    pub const fn attribute_count(&self) -> usize {
        self.attributes.len()
    }
}

impl VertexAttribute {
    /// Size in bytes.
    #[must_use]
    pub const fn byte_size(self) -> u32 {
        match self {
            Self::Position3F | Self::Normal3F => 12,
            Self::Uv2F => 8,
            Self::Color4F | Self::Tangent4F => 16,
        }
    }
}

// ---------------------------------------------------------------------------
// DrawCommand
// ---------------------------------------------------------------------------

/// A draw command for the renderer.
#[derive(Debug, Clone)]
pub struct DrawCommand {
    pub mesh_name: String,
    pub transform: Mat4,
    pub material_id: u32,
    pub instance_count: u32,
}

impl DrawCommand {
    #[must_use]
    pub fn new(mesh_name: &str, transform: Mat4, material_id: u32) -> Self {
        Self {
            mesh_name: mesh_name.to_string(),
            transform,
            material_id,
            instance_count: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// DrawQueue
// ---------------------------------------------------------------------------

/// Collects draw commands for a frame.
pub struct DrawQueue {
    commands: Vec<DrawCommand>,
}

impl DrawQueue {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn push(&mut self, cmd: DrawCommand) {
        self.commands.push(cmd);
    }

    /// Sorts by material for batching.
    pub fn sort_by_material(&mut self) {
        self.commands.sort_by_key(|c| c.material_id);
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    #[must_use]
    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.commands.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Total instances across all commands.
    #[must_use]
    pub fn total_instances(&self) -> u32 {
        self.commands.iter().map(|c| c.instance_count).sum()
    }
}

impl Default for DrawQueue {
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
    use crate::asset::Vertex;
    use crate::math::Mat4;

    #[test]
    fn gpu_mesh_desc_from_asset() {
        let verts = vec![
            Vertex::new([0.0; 3], [0.0; 3], [0.0; 2]),
            Vertex::new([1.0; 3], [0.0; 3], [0.0; 2]),
            Vertex::new([2.0; 3], [0.0; 3], [0.0; 2]),
        ];
        let indices = vec![0, 1, 2];
        let desc = GpuMeshDesc::from_asset("tri", &verts, &indices);
        assert_eq!(desc.vertex_count, 3);
        assert_eq!(desc.index_count, 3);
        assert_eq!(desc.triangle_count(), 1);
    }

    #[test]
    fn gpu_mesh_estimated_bytes() {
        let desc = GpuMeshDesc {
            name: "test".to_string(),
            vertex_count: 100,
            index_count: 300,
            vertex_stride: 32,
        };
        assert_eq!(desc.estimated_gpu_bytes(), 100 * 32 + 300 * 4);
    }

    #[test]
    fn vertex_layout_standard() {
        let layout = VertexLayout::standard();
        assert_eq!(layout.attribute_count(), 3);
        assert_eq!(layout.stride(), 12 + 12 + 8); // pos + normal + uv = 32
    }

    #[test]
    fn vertex_attribute_sizes() {
        assert_eq!(VertexAttribute::Position3F.byte_size(), 12);
        assert_eq!(VertexAttribute::Normal3F.byte_size(), 12);
        assert_eq!(VertexAttribute::Uv2F.byte_size(), 8);
        assert_eq!(VertexAttribute::Color4F.byte_size(), 16);
        assert_eq!(VertexAttribute::Tangent4F.byte_size(), 16);
    }

    #[test]
    fn draw_command_new() {
        let cmd = DrawCommand::new("cube", Mat4::IDENTITY, 0);
        assert_eq!(cmd.mesh_name, "cube");
        assert_eq!(cmd.instance_count, 1);
    }

    #[test]
    fn draw_queue_push_clear() {
        let mut q = DrawQueue::new();
        q.push(DrawCommand::new("a", Mat4::IDENTITY, 0));
        q.push(DrawCommand::new("b", Mat4::IDENTITY, 1));
        assert_eq!(q.len(), 2);
        q.clear();
        assert!(q.is_empty());
    }

    #[test]
    fn draw_queue_sort_by_material() {
        let mut q = DrawQueue::new();
        q.push(DrawCommand::new("a", Mat4::IDENTITY, 2));
        q.push(DrawCommand::new("b", Mat4::IDENTITY, 0));
        q.push(DrawCommand::new("c", Mat4::IDENTITY, 1));
        q.sort_by_material();
        assert_eq!(q.commands()[0].material_id, 0);
        assert_eq!(q.commands()[1].material_id, 1);
        assert_eq!(q.commands()[2].material_id, 2);
    }

    #[test]
    fn draw_queue_total_instances() {
        let mut q = DrawQueue::new();
        let mut cmd = DrawCommand::new("a", Mat4::IDENTITY, 0);
        cmd.instance_count = 5;
        q.push(cmd);
        q.push(DrawCommand::new("b", Mat4::IDENTITY, 0));
        assert_eq!(q.total_instances(), 6);
    }

    #[test]
    fn draw_queue_default() {
        let q = DrawQueue::default();
        assert!(q.is_empty());
    }
}
