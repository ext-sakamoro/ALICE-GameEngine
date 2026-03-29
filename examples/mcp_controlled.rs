//! MCP Controlled: demonstrates engine control via MCP JSON-RPC.
//!
//! Run: `cargo run --example mcp_controlled`

use alice_game_engine::engine::EngineContext;
use alice_game_engine::mcp::{McpHandler, McpRequest};

fn call(ctx: &mut EngineContext, method: &str, params: serde_json::Value) -> serde_json::Value {
    let req = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: method.to_string(),
        params,
    };
    let resp = McpHandler::handle(&req, ctx);
    resp.result
        .unwrap_or(serde_json::json!({"error": resp.error}))
}

fn main() {
    let mut ctx = EngineContext::new();

    // List tools
    let tools = call(&mut ctx, "tools/list", serde_json::json!({}));
    let tool_count = tools["tools"].as_array().map_or(0, |a| a.len());
    println!("Available MCP tools: {tool_count}");

    // Add a scene via MCP
    call(
        &mut ctx,
        "tools/call",
        serde_json::json!({"name": "scene_add_node", "arguments": {"name": "camera", "kind": "camera"}}),
    );
    call(
        &mut ctx,
        "tools/call",
        serde_json::json!({"name": "scene_add_node", "arguments": {"name": "cube", "kind": "mesh", "x": 0, "y": 1, "z": -5}}),
    );
    call(
        &mut ctx,
        "tools/call",
        serde_json::json!({"name": "scene_add_node", "arguments": {"name": "light", "kind": "light", "x": 0, "y": 10, "z": 0}}),
    );
    call(
        &mut ctx,
        "tools/call",
        serde_json::json!({"name": "scene_add_node", "arguments": {"name": "sphere", "kind": "sdf", "x": 3, "y": 0, "z": 0}}),
    );

    // Check status
    let status = call(
        &mut ctx,
        "tools/call",
        serde_json::json!({"name": "engine_status", "arguments": {}}),
    );
    println!("Engine status: {status}");

    // List scene
    let scene = call(
        &mut ctx,
        "tools/call",
        serde_json::json!({"name": "scene_list", "arguments": {}}),
    );
    if let Some(nodes) = scene["nodes"].as_array() {
        println!("\nScene ({} nodes):", nodes.len());
        for node in nodes {
            println!("  [{:>2}] {} ({})", node["id"], node["name"], node["kind"]);
        }
    }

    // Move the cube
    call(
        &mut ctx,
        "tools/call",
        serde_json::json!({"name": "scene_set_transform", "arguments": {"id": 1, "x": 5, "y": 2, "z": -3}}),
    );
    println!("\nCube moved to (5, 2, -3)");

    // Step physics
    call(
        &mut ctx,
        "tools/call",
        serde_json::json!({"name": "physics_step", "arguments": {"frames": 60}}),
    );

    let status = call(
        &mut ctx,
        "tools/call",
        serde_json::json!({"name": "engine_status", "arguments": {}}),
    );
    println!("After 60 physics steps: {status}");
}
