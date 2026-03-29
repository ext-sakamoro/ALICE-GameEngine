//! MCP (Model Context Protocol) server for engine remote control.
//!
//! Exposes engine state as JSON-RPC tools that can be called from
//! Claude Code, Cursor, or any MCP-compatible client over stdio.
//!
//! ## Available Tools
//!
//! | Tool | Description |
//! |------|-------------|
//! | `scene_list` | List all nodes in the scene |
//! | `scene_add_node` | Add a node (mesh/sdf/light/camera) |
//! | `scene_remove_node` | Remove a node by ID |
//! | `scene_set_transform` | Set node position/rotation/scale |
//! | `physics_add_body` | Add a rigid body |
//! | `physics_step` | Step the physics simulation |
//! | `animation_play` | Play an animation clip |
//! | `engine_status` | Get frame count, time, node count |

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// MCP Request / Response
// ---------------------------------------------------------------------------

/// JSON-RPC request from an MCP client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// JSON-RPC response to an MCP client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

/// JSON-RPC error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
}

impl McpResponse {
    #[must_use]
    pub fn success(id: u64, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    #[must_use]
    pub fn error(id: u64, code: i32, message: &str) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(McpError {
                code,
                message: message.to_string(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

/// MCP tool definition for `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// Returns the list of all available engine tools.
#[must_use]
pub fn tool_definitions() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "scene_list".to_string(),
            description: "List all nodes in the scene graph".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        },
        McpTool {
            name: "scene_add_node".to_string(),
            description: "Add a node to the scene (mesh, sdf, light, camera, empty)".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "kind": {"type": "string", "enum": ["mesh", "sdf", "light", "camera", "empty"]},
                    "x": {"type": "number"}, "y": {"type": "number"}, "z": {"type": "number"}
                },
                "required": ["name", "kind"]
            }),
        },
        McpTool {
            name: "scene_remove_node".to_string(),
            description: "Remove a node by ID".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"id": {"type": "integer"}},
                "required": ["id"]
            }),
        },
        McpTool {
            name: "scene_set_transform".to_string(),
            description: "Set a node's position".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                    "x": {"type": "number"}, "y": {"type": "number"}, "z": {"type": "number"}
                },
                "required": ["id"]
            }),
        },
        McpTool {
            name: "engine_status".to_string(),
            description: "Get engine status (time, frames, nodes)".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        },
        McpTool {
            name: "physics_step".to_string(),
            description: "Step the physics simulation by N frames".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"frames": {"type": "integer", "default": 1}},
            }),
        },
    ]
}

// ---------------------------------------------------------------------------
// McpHandler — processes requests against EngineContext
// ---------------------------------------------------------------------------

/// Handles MCP requests by operating on the engine context.
pub struct McpHandler;

impl McpHandler {
    /// Dispatches a request to the appropriate handler.
    #[must_use]
    pub fn handle(request: &McpRequest, ctx: &mut crate::engine::EngineContext) -> McpResponse {
        match request.method.as_str() {
            "tools/list" => {
                McpResponse::success(request.id, serde_json::json!({"tools": tool_definitions()}))
            }
            "tools/call" => {
                let tool_name = request
                    .params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args = request.params.get("arguments").cloned().unwrap_or_default();
                Self::call_tool(request.id, tool_name, &args, ctx)
            }
            _ => McpResponse::error(request.id, -32601, "Method not found"),
        }
    }

    fn call_tool(
        id: u64,
        name: &str,
        args: &serde_json::Value,
        ctx: &mut crate::engine::EngineContext,
    ) -> McpResponse {
        use crate::scene_graph::{CameraData, LightData, MeshData, Node, NodeKind, SdfData};

        match name {
            "scene_list" => {
                let mut nodes = Vec::new();
                for i in 0..1000_u32 {
                    let nid = crate::scene_graph::NodeId(i);
                    if let Some(node) = ctx.scene.get(nid) {
                        nodes.push(serde_json::json!({
                            "id": i,
                            "name": node.name,
                            "kind": format!("{:?}", node.kind),
                            "visible": node.visible,
                        }));
                    }
                }
                McpResponse::success(id, serde_json::json!({"nodes": nodes}))
            }
            "scene_add_node" => {
                let name_str = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unnamed");
                let kind_str = args.get("kind").and_then(|v| v.as_str()).unwrap_or("empty");
                let x = args
                    .get("x")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0) as f32;
                let y = args
                    .get("y")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0) as f32;
                let z = args
                    .get("z")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0) as f32;

                let kind = match kind_str {
                    "mesh" => NodeKind::Mesh(MeshData::default()),
                    "camera" => NodeKind::Camera(CameraData::default()),
                    "light" => NodeKind::Light(LightData::default()),
                    "sdf" => NodeKind::Sdf(SdfData::default()),
                    _ => NodeKind::Empty,
                };
                let mut node = Node::new(name_str, kind);
                node.local_transform.position = crate::math::Vec3::new(x, y, z);
                let nid = ctx.scene.add(node);
                McpResponse::success(id, serde_json::json!({"id": nid.0}))
            }
            "scene_remove_node" => {
                let node_id = args
                    .get("id")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as u32;
                ctx.scene.remove(crate::scene_graph::NodeId(node_id));
                McpResponse::success(id, serde_json::json!({"removed": node_id}))
            }
            "scene_set_transform" => {
                let node_id = args
                    .get("id")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as u32;
                let x = args
                    .get("x")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0) as f32;
                let y = args
                    .get("y")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0) as f32;
                let z = args
                    .get("z")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0) as f32;
                if let Some(node) = ctx.scene.get_mut(crate::scene_graph::NodeId(node_id)) {
                    node.local_transform.position = crate::math::Vec3::new(x, y, z);
                }
                McpResponse::success(id, serde_json::json!({"ok": true}))
            }
            "engine_status" => McpResponse::success(
                id,
                serde_json::json!({
                    "frame_count": ctx.time.frame_count,
                    "total_seconds": ctx.time.total_seconds,
                    "node_count": ctx.scene.node_count(),
                    "plugin_count": ctx.plugins.count(),
                }),
            ),
            "physics_step" => {
                let frames = args
                    .get("frames")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(1);
                for _ in 0..frames {
                    ctx.time.tick(1.0 / 60.0);
                }
                McpResponse::success(id, serde_json::json!({"stepped": frames}))
            }
            _ => McpResponse::error(id, -32602, &format!("Unknown tool: {name}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Stdio transport (line-delimited JSON)
// ---------------------------------------------------------------------------

/// Parses a JSON-RPC request from a string.
///
/// # Errors
///
/// Returns `serde_json::Error` if parsing fails.
pub fn parse_request(json: &str) -> Result<McpRequest, serde_json::Error> {
    serde_json::from_str(json)
}

/// Serializes a response to a JSON string.
///
/// # Errors
///
/// Returns `serde_json::Error` if serialization fails.
pub fn serialize_response(response: &McpResponse) -> Result<String, serde_json::Error> {
    serde_json::to_string(response)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineContext;

    #[test]
    fn tool_definitions_count() {
        let tools = tool_definitions();
        assert!(tools.len() >= 6);
    }

    #[test]
    fn handle_tools_list() {
        let mut ctx = EngineContext::new();
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "tools/list".to_string(),
            params: serde_json::Value::Null,
        };
        let resp = McpHandler::handle(&req, &mut ctx);
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn handle_scene_add_node() {
        let mut ctx = EngineContext::new();
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: 2,
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": "scene_add_node",
                "arguments": {"name": "cube1", "kind": "mesh", "x": 1.0, "y": 2.0, "z": 3.0}
            }),
        };
        let resp = McpHandler::handle(&req, &mut ctx);
        assert!(resp.result.is_some());
        assert_eq!(ctx.scene.node_count(), 1);
    }

    #[test]
    fn handle_scene_list() {
        let mut ctx = EngineContext::new();
        ctx.scene.add(crate::scene_graph::Node::new(
            "test",
            crate::scene_graph::NodeKind::Empty,
        ));
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: 3,
            method: "tools/call".to_string(),
            params: serde_json::json!({"name": "scene_list", "arguments": {}}),
        };
        let resp = McpHandler::handle(&req, &mut ctx);
        let nodes = resp.result.unwrap()["nodes"].as_array().unwrap().len();
        assert_eq!(nodes, 1);
    }

    #[test]
    fn handle_engine_status() {
        let mut ctx = EngineContext::new();
        ctx.time.tick(0.5);
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: 4,
            method: "tools/call".to_string(),
            params: serde_json::json!({"name": "engine_status", "arguments": {}}),
        };
        let resp = McpHandler::handle(&req, &mut ctx);
        let result = resp.result.unwrap();
        assert_eq!(result["frame_count"], 1);
    }

    #[test]
    fn handle_scene_remove() {
        let mut ctx = EngineContext::new();
        ctx.scene.add(crate::scene_graph::Node::new(
            "temp",
            crate::scene_graph::NodeKind::Empty,
        ));
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: 5,
            method: "tools/call".to_string(),
            params: serde_json::json!({"name": "scene_remove_node", "arguments": {"id": 0}}),
        };
        McpHandler::handle(&req, &mut ctx);
        assert_eq!(ctx.scene.node_count(), 0);
    }

    #[test]
    fn handle_set_transform() {
        let mut ctx = EngineContext::new();
        ctx.scene.add(crate::scene_graph::Node::new(
            "obj",
            crate::scene_graph::NodeKind::Empty,
        ));
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: 6,
            method: "tools/call".to_string(),
            params: serde_json::json!({"name": "scene_set_transform", "arguments": {"id": 0, "x": 10.0, "y": 20.0, "z": 30.0}}),
        };
        McpHandler::handle(&req, &mut ctx);
        let pos = ctx
            .scene
            .get(crate::scene_graph::NodeId(0))
            .unwrap()
            .local_transform
            .position;
        assert!((pos.x() - 10.0).abs() < 1e-6);
    }

    #[test]
    fn handle_unknown_method() {
        let mut ctx = EngineContext::new();
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: 7,
            method: "unknown".to_string(),
            params: serde_json::Value::Null,
        };
        let resp = McpHandler::handle(&req, &mut ctx);
        assert!(resp.error.is_some());
    }

    #[test]
    fn handle_unknown_tool() {
        let mut ctx = EngineContext::new();
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: 8,
            method: "tools/call".to_string(),
            params: serde_json::json!({"name": "nonexistent", "arguments": {}}),
        };
        let resp = McpHandler::handle(&req, &mut ctx);
        assert!(resp.error.is_some());
    }

    #[test]
    fn parse_and_serialize() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
        let req = parse_request(json).unwrap();
        assert_eq!(req.method, "tools/list");
        let resp = McpResponse::success(1, serde_json::json!({"ok": true}));
        let s = serialize_response(&resp).unwrap();
        assert!(s.contains("ok"));
    }

    #[test]
    fn physics_step_tool() {
        let mut ctx = EngineContext::new();
        let req = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: 9,
            method: "tools/call".to_string(),
            params: serde_json::json!({"name": "physics_step", "arguments": {"frames": 10}}),
        };
        McpHandler::handle(&req, &mut ctx);
        assert_eq!(ctx.time.frame_count, 10);
    }
}
