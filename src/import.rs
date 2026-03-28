//! Project import: read Unity (.unity, .prefab) and UE5 (.umap, .uasset)
//! scene files into the ALICE scene graph.
//!
//! This module parses the binary/text headers and extracts transform
//! hierarchies, mesh references, and light data into ALICE `Node` trees.

use crate::math::{Quat, Vec3};
use crate::scene_graph::{CameraData, LightData, LocalTransform, MeshData, Node, NodeKind};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Detected project format
// ---------------------------------------------------------------------------

/// Detected engine project format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectFormat {
    UnityScene,
    UnityPrefab,
    UnrealMap,
    UnrealAsset,
    AliceScene,
    Unknown,
}

/// Detects the project format from file extension.
#[must_use]
pub fn detect_format(path: &str) -> ProjectFormat {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "unity" => ProjectFormat::UnityScene,
        "prefab" => ProjectFormat::UnityPrefab,
        "umap" => ProjectFormat::UnrealMap,
        "uasset" => ProjectFormat::UnrealAsset,
        "scene" | "alice" => ProjectFormat::AliceScene,
        _ => ProjectFormat::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Unity YAML scene parser (simplified)
// ---------------------------------------------------------------------------

/// A parsed Unity `GameObject` entry.
#[derive(Debug, Clone)]
pub struct UnityGameObject {
    pub name: String,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub components: Vec<UnityComponent>,
    pub children: Vec<Self>,
}

/// Known Unity component types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnityComponent {
    MeshRenderer { mesh_name: String },
    Camera { fov: f32 },
    Light { light_type: String, intensity: f32 },
    Collider { shape: String },
    Unknown(String),
}

/// Parses Unity YAML scene text into `UnityGameObject` entries.
/// This handles the basic `GameObject` + `Transform` structure.
#[must_use]
pub fn parse_unity_yaml(yaml_text: &str) -> Vec<UnityGameObject> {
    let mut objects = Vec::new();
    let mut current_name = String::new();
    let mut pos = Vec3::ZERO;
    let mut rot = Quat::IDENTITY;
    let mut scale = Vec3::ONE;
    let mut components: Vec<UnityComponent> = Vec::new();
    let mut in_gameobject = false;

    for line in yaml_text.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("--- !u!1") {
            if in_gameobject && !current_name.is_empty() {
                objects.push(UnityGameObject {
                    name: current_name.clone(),
                    position: pos,
                    rotation: rot,
                    scale,
                    components: components.clone(),
                    children: Vec::new(),
                });
            }
            current_name.clear();
            pos = Vec3::ZERO;
            rot = Quat::IDENTITY;
            scale = Vec3::ONE;
            components.clear();
            in_gameobject = true;
        }

        if let Some(name) = trimmed.strip_prefix("m_Name: ") {
            current_name = name.to_string();
        }

        if let Some(pos_str) = trimmed.strip_prefix("m_LocalPosition: {x: ") {
            if let Some(parsed) = parse_vec3_yaml(pos_str) {
                pos = parsed;
            }
        }

        if let Some(scale_str) = trimmed.strip_prefix("m_LocalScale: {x: ") {
            if let Some(parsed) = parse_vec3_yaml(scale_str) {
                scale = parsed;
            }
        }

        if trimmed.contains("MeshRenderer") {
            components.push(UnityComponent::MeshRenderer {
                mesh_name: current_name.clone(),
            });
        }
        if trimmed.contains("Camera") && !trimmed.contains("m_") {
            components.push(UnityComponent::Camera { fov: 60.0 });
        }
    }

    if in_gameobject && !current_name.is_empty() {
        objects.push(UnityGameObject {
            name: current_name,
            position: pos,
            rotation: rot,
            scale,
            components,
            children: Vec::new(),
        });
    }

    objects
}

fn parse_vec3_yaml(s: &str) -> Option<Vec3> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() < 3 {
        return None;
    }
    let x: f32 = parts[0].trim().trim_end_matches('}').parse().ok()?;
    let y: f32 = parts[1]
        .trim()
        .trim_start_matches("y: ")
        .trim_end_matches('}')
        .parse()
        .ok()?;
    let z: f32 = parts[2]
        .trim()
        .trim_start_matches("z: ")
        .trim_end_matches('}')
        .parse()
        .ok()?;
    Some(Vec3::new(x, y, z))
}

// ---------------------------------------------------------------------------
// UE5 .uasset header parser
// ---------------------------------------------------------------------------

/// Minimal UE5 .uasset header.
#[derive(Debug, Clone, Copy)]
pub struct UassetHeader {
    pub magic: u32,
    pub legacy_version: i32,
    pub name_count: u32,
    pub export_count: u32,
}

/// Parses a UE5 .uasset binary header.
#[must_use]
pub fn parse_uasset_header(data: &[u8]) -> Option<UassetHeader> {
    if data.len() < 24 {
        return None;
    }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    // UE4/5 magic: 0x9E2A83C1
    if magic != 0x9E2A_83C1 {
        return None;
    }
    let legacy_version = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let name_count = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    let export_count = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
    Some(UassetHeader {
        magic,
        legacy_version,
        name_count,
        export_count,
    })
}

// ---------------------------------------------------------------------------
// Convert to ALICE scene graph nodes
// ---------------------------------------------------------------------------

/// Converts a Unity `GameObject` to an ALICE Node.
#[must_use]
pub fn unity_to_node(obj: &UnityGameObject) -> Node {
    let kind = if obj
        .components
        .iter()
        .any(|c| matches!(c, UnityComponent::Camera { .. }))
    {
        NodeKind::Camera(CameraData::default())
    } else if obj
        .components
        .iter()
        .any(|c| matches!(c, UnityComponent::Light { .. }))
    {
        NodeKind::Light(LightData::default())
    } else if obj
        .components
        .iter()
        .any(|c| matches!(c, UnityComponent::MeshRenderer { .. }))
    {
        NodeKind::Mesh(MeshData::default())
    } else {
        NodeKind::Empty
    };

    let mut node = Node::new(&obj.name, kind);
    node.local_transform = LocalTransform {
        position: obj.position,
        rotation: obj.rotation,
        scale: obj.scale,
    };
    node
}

/// Converts all Unity `GameObjects` to ALICE Nodes.
#[must_use]
pub fn unity_scene_to_nodes(objects: &[UnityGameObject]) -> Vec<Node> {
    objects.iter().map(unity_to_node).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_unity_scene() {
        assert_eq!(detect_format("level.unity"), ProjectFormat::UnityScene);
    }

    #[test]
    fn detect_unity_prefab() {
        assert_eq!(detect_format("player.prefab"), ProjectFormat::UnityPrefab);
    }

    #[test]
    fn detect_unreal_map() {
        assert_eq!(detect_format("map.umap"), ProjectFormat::UnrealMap);
    }

    #[test]
    fn detect_unreal_asset() {
        assert_eq!(detect_format("mesh.uasset"), ProjectFormat::UnrealAsset);
    }

    #[test]
    fn detect_alice_scene() {
        assert_eq!(detect_format("world.alice"), ProjectFormat::AliceScene);
    }

    #[test]
    fn detect_unknown() {
        assert_eq!(detect_format("readme.txt"), ProjectFormat::Unknown);
    }

    #[test]
    fn parse_unity_yaml_basic() {
        let yaml = r#"
--- !u!1 &100
GameObject:
  m_Name: MainCamera
--- !u!4 &200
Transform:
  m_LocalPosition: {x: 0, y: 1, z: -10}
  m_LocalScale: {x: 1, y: 1, z: 1}
--- !u!1 &300
GameObject:
  m_Name: Cube
  MeshRenderer:
"#;
        let objects = parse_unity_yaml(yaml);
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].name, "MainCamera");
        assert_eq!(objects[1].name, "Cube");
    }

    #[test]
    fn parse_unity_position() {
        let yaml = r#"
--- !u!1 &1
GameObject:
  m_Name: Obj
  m_LocalPosition: {x: 3.5, y: 2.0, z: -1.0}
"#;
        let objects = parse_unity_yaml(yaml);
        assert_eq!(objects.len(), 1);
        assert!((objects[0].position.x() - 3.5).abs() < 1e-4);
    }

    #[test]
    fn parse_unity_empty() {
        let objects = parse_unity_yaml("");
        assert!(objects.is_empty());
    }

    #[test]
    fn uasset_header_valid() {
        let mut data = vec![0u8; 28];
        // Magic: 0x9E2A83C1
        data[0..4].copy_from_slice(&0x9E2A_83C1_u32.to_le_bytes());
        data[4..8].copy_from_slice(&(-7_i32).to_le_bytes());
        data[16..20].copy_from_slice(&42_u32.to_le_bytes());
        data[24..28].copy_from_slice(&10_u32.to_le_bytes());
        let header = parse_uasset_header(&data).unwrap();
        assert_eq!(header.name_count, 42);
        assert_eq!(header.export_count, 10);
    }

    #[test]
    fn uasset_header_invalid_magic() {
        let data = vec![0u8; 28];
        assert!(parse_uasset_header(&data).is_none());
    }

    #[test]
    fn uasset_header_too_short() {
        let data = vec![0u8; 10];
        assert!(parse_uasset_header(&data).is_none());
    }

    #[test]
    fn unity_to_node_mesh() {
        let obj = UnityGameObject {
            name: "Cube".to_string(),
            position: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            components: vec![UnityComponent::MeshRenderer {
                mesh_name: "Cube".to_string(),
            }],
            children: Vec::new(),
        };
        let node = unity_to_node(&obj);
        assert_eq!(node.name, "Cube");
        assert!(matches!(node.kind, NodeKind::Mesh(_)));
        assert!((node.local_transform.position.x() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn unity_to_node_camera() {
        let obj = UnityGameObject {
            name: "Cam".to_string(),
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            components: vec![UnityComponent::Camera { fov: 60.0 }],
            children: Vec::new(),
        };
        let node = unity_to_node(&obj);
        assert!(matches!(node.kind, NodeKind::Camera(_)));
    }

    #[test]
    fn unity_scene_to_nodes_multiple() {
        let objects = vec![
            UnityGameObject {
                name: "A".to_string(),
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
                components: vec![],
                children: vec![],
            },
            UnityGameObject {
                name: "B".to_string(),
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
                components: vec![],
                children: vec![],
            },
        ];
        let nodes = unity_scene_to_nodes(&objects);
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn parse_vec3_yaml_basic() {
        let v = parse_vec3_yaml("1.5, y: 2.0, z: 3.0}").unwrap();
        assert!((v.x() - 1.5).abs() < 1e-4);
        assert!((v.y() - 2.0).abs() < 1e-4);
    }

    #[test]
    fn parse_vec3_yaml_invalid() {
        assert!(parse_vec3_yaml("not numbers").is_none());
    }
}
