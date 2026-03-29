//! SDF Asset loader for Open-Source-SDF-Assets (.asdf.json) format.
//!
//! Loads 991 free SDF assets from:
//! <https://github.com/ext-sakamoro/Open-Source-SDF-Assets>
//!
//! These are SDF descriptions converted from
//! [Open Source 3D Assets](https://www.opensource3dassets.com/) via ALICE-SDF.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ASDF format
// ---------------------------------------------------------------------------

/// Header for an `.asdf.json` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsdfFile {
    pub version: String,
    pub root: serde_json::Value,
}

/// Metadata for an SDF asset in the collection index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsdfIndex {
    pub name: String,
    pub path: String,
    pub collection: String,
}

/// Loads an `.asdf.json` file and returns the root SDF node JSON.
///
/// # Errors
///
/// Returns an error if the file contents are not valid ASDF JSON.
pub fn load_asdf(json_text: &str) -> Result<AsdfFile, serde_json::Error> {
    serde_json::from_str(json_text)
}

/// Converts an ASDF root node to an ALICE-GameEngine `SdfData` for the scene graph.
#[must_use]
pub fn asdf_to_sdf_data(asdf: &AsdfFile) -> crate::scene_graph::SdfData {
    let json_str = serde_json::to_string(&asdf.root).unwrap_or_default();
    crate::scene_graph::SdfData {
        sdf_json: json_str,
        half_extents: crate::math::Vec3::new(2.0, 2.0, 2.0),
        generate_collider: false,
    }
}

/// Adds an SDF asset to the scene graph as a node.
pub fn add_asdf_to_scene(
    scene: &mut crate::scene_graph::SceneGraph,
    name: &str,
    asdf: &AsdfFile,
    position: crate::math::Vec3,
) -> crate::scene_graph::NodeId {
    let sdf_data = asdf_to_sdf_data(asdf);
    let mut node = crate::scene_graph::Node::new(name, crate::scene_graph::NodeKind::Sdf(sdf_data));
    node.local_transform.position = position;
    scene.add(node)
}

/// Scans a directory for `.asdf.json` files and returns their paths.
#[must_use]
pub fn scan_asdf_directory(dir_path: &str) -> Vec<String> {
    let mut paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".asdf.json") {
                    paths.push(path.to_string_lossy().to_string());
                }
            }
        }
    }
    paths
}

/// Loads an ASDF file from disk.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed.
pub fn load_asdf_file(path: &str) -> Result<AsdfFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Read error: {e}"))?;
    load_asdf(&text).map_err(|e| format!("Parse error: {e}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_ASDF: &str = r#"{
        "version": "0.1.0",
        "root": {
            "Union": {
                "a": {"Sphere": {"radius": 1.0, "center": [0,0,0]}},
                "b": {"Sphere": {"radius": 0.5, "center": [1,0,0]}}
            }
        }
    }"#;

    #[test]
    fn load_asdf_valid() {
        let asdf = load_asdf(SAMPLE_ASDF).unwrap();
        assert_eq!(asdf.version, "0.1.0");
        assert!(asdf.root.is_object());
    }

    #[test]
    fn load_asdf_invalid() {
        assert!(load_asdf("not json").is_err());
    }

    #[test]
    fn asdf_to_sdf_data_converts() {
        let asdf = load_asdf(SAMPLE_ASDF).unwrap();
        let sdf_data = asdf_to_sdf_data(&asdf);
        assert!(!sdf_data.sdf_json.is_empty());
        assert!(sdf_data.sdf_json.contains("Union"));
    }

    #[test]
    fn add_to_scene() {
        let asdf = load_asdf(SAMPLE_ASDF).unwrap();
        let mut scene = crate::scene_graph::SceneGraph::new("test");
        let id = add_asdf_to_scene(&mut scene, "bench", &asdf, crate::math::Vec3::ZERO);
        assert_eq!(scene.node_count(), 1);
        let node = scene.get(id).unwrap();
        assert_eq!(node.name, "bench");
    }

    #[test]
    fn scan_nonexistent_dir() {
        let paths = scan_asdf_directory("/nonexistent/path");
        assert!(paths.is_empty());
    }

    #[test]
    fn load_asdf_file_missing() {
        assert!(load_asdf_file("/nonexistent.asdf.json").is_err());
    }

    #[test]
    fn asdf_index_struct() {
        let idx = AsdfIndex {
            name: "Bench".to_string(),
            path: "collections/pm-momuspark/Bench_01_Art.asdf.json".to_string(),
            collection: "pm-momuspark".to_string(),
        };
        assert_eq!(idx.name, "Bench");
    }
}
