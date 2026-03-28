//! Asset pipeline: extensible loaders for meshes, textures, SDF, and more.

use crate::math::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Vertex / MeshAsset
// ---------------------------------------------------------------------------

/// A vertex with position, normal, UV.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex {
    #[must_use]
    pub const fn new(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            uv,
        }
    }
}

/// A loaded mesh asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshAsset {
    pub name: String,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshAsset {
    #[must_use]
    pub const fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    #[must_use]
    pub const fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Computes AABB from vertex positions.
    #[must_use]
    pub fn compute_aabb(&self) -> (Vec3, Vec3) {
        if self.vertices.is_empty() {
            return (Vec3::ZERO, Vec3::ZERO);
        }
        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for v in &self.vertices {
            let p = v.position;
            min = Vec3::new(min.x().min(p[0]), min.y().min(p[1]), min.z().min(p[2]));
            max = Vec3::new(max.x().max(p[0]), max.y().max(p[1]), max.z().max(p[2]));
        }
        (min, max)
    }
}

// ---------------------------------------------------------------------------
// OBJ parser (minimal)
// ---------------------------------------------------------------------------

/// Parses a minimal OBJ string into a `MeshAsset`.
/// Supports `v` (position) and `f` (face, triangles only) lines.
#[must_use]
pub fn parse_obj(name: &str, obj_text: &str) -> MeshAsset {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for line in obj_text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("v ") {
            let parts: Vec<f32> = rest
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            if parts.len() >= 3 {
                positions.push([parts[0], parts[1], parts[2]]);
            }
        } else if let Some(rest) = line.strip_prefix("f ") {
            let face_indices: Vec<u32> = rest
                .split_whitespace()
                .filter_map(|s| {
                    let idx_str = s.split('/').next()?;
                    idx_str.parse::<u32>().ok().map(|i| i - 1) // OBJ is 1-indexed
                })
                .collect();
            // Triangulate fan
            if face_indices.len() >= 3 {
                for i in 1..face_indices.len() - 1 {
                    for &fi in &[face_indices[0], face_indices[i], face_indices[i + 1]] {
                        let pos = positions.get(fi as usize).copied().unwrap_or([0.0; 3]);
                        let vi = vertices.len() as u32;
                        vertices.push(Vertex::new(pos, [0.0, 1.0, 0.0], [0.0, 0.0]));
                        indices.push(vi);
                    }
                }
            }
        }
    }

    MeshAsset {
        name: name.to_string(),
        vertices,
        indices,
    }
}

// ---------------------------------------------------------------------------
// SDF Asset (JSON)
// ---------------------------------------------------------------------------

/// Loads an SDF node tree from JSON.
///
/// # Errors
///
/// Returns `serde_json::Error` if the JSON is malformed.
pub fn load_sdf_json(json: &str) -> Result<crate::sdf::SdfNode, serde_json::Error> {
    serde_json::from_str(json)
}

// ---------------------------------------------------------------------------
// AssetType
// ---------------------------------------------------------------------------

/// Recognized asset types by file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetType {
    Mesh,
    Texture,
    Sound,
    Sdf,
    Animation,
    Scene,
    Unknown,
}

/// Determines asset type from a file path extension.
#[must_use]
pub fn asset_type_from_path(path: &str) -> AssetType {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "obj" | "gltf" | "glb" | "fbx" => AssetType::Mesh,
        "png" | "jpg" | "jpeg" | "bmp" | "tga" | "hdr" => AssetType::Texture,
        "wav" | "ogg" | "mp3" | "flac" => AssetType::Sound,
        "sdf" | "sdf.json" => AssetType::Sdf,
        "anim" | "anim.json" => AssetType::Animation,
        "scene" | "scene.json" => AssetType::Scene,
        _ => AssetType::Unknown,
    }
}

// ---------------------------------------------------------------------------
// glTF header parsing (minimal)
// ---------------------------------------------------------------------------

/// Minimal glTF binary header.
#[derive(Debug, Clone, Copy)]
pub struct GltfHeader {
    pub magic: u32,
    pub version: u32,
    pub length: u32,
}

/// Parses a glTF binary (.glb) header from bytes.
#[must_use]
pub fn parse_glb_header(data: &[u8]) -> Option<GltfHeader> {
    if data.len() < 12 {
        return None;
    }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let length = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    if magic != 0x4654_6C67 {
        // "glTF" in little-endian
        return None;
    }
    Some(GltfHeader {
        magic,
        version,
        length,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_new() {
        let v = Vertex::new([1.0, 2.0, 3.0], [0.0, 1.0, 0.0], [0.5, 0.5]);
        assert_eq!(v.position, [1.0, 2.0, 3.0]);
        assert_eq!(v.uv, [0.5, 0.5]);
    }

    #[test]
    fn parse_obj_cube() {
        let obj = "\
v -1.0 -1.0 -1.0
v  1.0 -1.0 -1.0
v  1.0  1.0 -1.0
v -1.0  1.0 -1.0
f 1 2 3
f 1 3 4
";
        let mesh = parse_obj("cube", obj);
        assert_eq!(mesh.name, "cube");
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.vertex_count(), 6);
    }

    #[test]
    fn parse_obj_empty() {
        let mesh = parse_obj("empty", "");
        assert_eq!(mesh.vertex_count(), 0);
        assert_eq!(mesh.triangle_count(), 0);
    }

    #[test]
    fn parse_obj_quad_fan() {
        let obj = "\
v 0 0 0
v 1 0 0
v 1 1 0
v 0 1 0
f 1 2 3 4
";
        let mesh = parse_obj("quad", obj);
        assert_eq!(mesh.triangle_count(), 2);
    }

    #[test]
    fn parse_obj_with_normals_uvs() {
        let obj = "\
v 0 0 0
v 1 0 0
v 0 1 0
vn 0 0 1
vt 0 0
f 1/1/1 2/1/1 3/1/1
";
        let mesh = parse_obj("tri", obj);
        assert_eq!(mesh.triangle_count(), 1);
    }

    #[test]
    fn mesh_asset_aabb() {
        let mesh = MeshAsset {
            name: "test".to_string(),
            vertices: vec![
                Vertex::new([-1.0, -2.0, -3.0], [0.0; 3], [0.0; 2]),
                Vertex::new([1.0, 2.0, 3.0], [0.0; 3], [0.0; 2]),
            ],
            indices: vec![],
        };
        let (min, max) = mesh.compute_aabb();
        assert!((min.x() - (-1.0)).abs() < 1e-6);
        assert!((max.z() - 3.0).abs() < 1e-6);
    }

    #[test]
    fn mesh_asset_empty_aabb() {
        let mesh = MeshAsset {
            name: "empty".to_string(),
            vertices: vec![],
            indices: vec![],
        };
        let (min, max) = mesh.compute_aabb();
        assert_eq!(min, Vec3::ZERO);
        assert_eq!(max, Vec3::ZERO);
    }

    #[test]
    fn asset_type_detection() {
        assert_eq!(asset_type_from_path("model.obj"), AssetType::Mesh);
        assert_eq!(asset_type_from_path("model.gltf"), AssetType::Mesh);
        assert_eq!(asset_type_from_path("tex.png"), AssetType::Texture);
        assert_eq!(asset_type_from_path("sound.wav"), AssetType::Sound);
        assert_eq!(asset_type_from_path("tree.sdf"), AssetType::Sdf);
        assert_eq!(asset_type_from_path("walk.anim"), AssetType::Animation);
        assert_eq!(asset_type_from_path("level.scene"), AssetType::Scene);
        assert_eq!(asset_type_from_path("readme.txt"), AssetType::Unknown);
    }

    #[test]
    fn glb_header_valid() {
        // "glTF" magic = 0x46546C67
        let data: Vec<u8> = vec![
            0x67, 0x6C, 0x54, 0x46, // magic
            0x02, 0x00, 0x00, 0x00, // version 2
            0x00, 0x01, 0x00, 0x00, // length 256
        ];
        let header = parse_glb_header(&data).unwrap();
        assert_eq!(header.version, 2);
        assert_eq!(header.length, 256);
    }

    #[test]
    fn glb_header_invalid() {
        let data = vec![0u8; 12];
        assert!(parse_glb_header(&data).is_none());
    }

    #[test]
    fn glb_header_too_short() {
        let data = vec![0u8; 4];
        assert!(parse_glb_header(&data).is_none());
    }

    #[test]
    fn sdf_json_load() {
        let json = r#"{"Primitive":{"Sphere":{"radius":2.5}}}"#;
        let node = load_sdf_json(json).unwrap();
        assert!(node.eval(crate::math::Vec3::ZERO) < 0.0);
    }

    #[test]
    fn sdf_json_invalid() {
        let result = load_sdf_json("not json");
        assert!(result.is_err());
    }
}
