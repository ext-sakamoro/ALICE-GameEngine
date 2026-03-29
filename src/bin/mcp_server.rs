//! ALICE-GameEngine MCP server.
//!
//! Reads JSON-RPC requests from stdin, operates on the engine, writes responses to stdout.
//!
//! Register in Claude Code:
//! ```bash
//! claude mcp add --transport stdio alice-engine -- cargo run --bin mcp_server --features full
//! ```

use alice_game_engine::engine::EngineContext;
use alice_game_engine::mcp::{self, McpHandler};
use std::io::{self, BufRead, Write};

fn main() {
    let mut ctx = EngineContext::new();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    eprintln!("ALICE-GameEngine MCP server started. Waiting for JSON-RPC on stdin...");

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = match mcp::parse_request(trimmed) {
            Ok(request) => McpHandler::handle(&request, &mut ctx),
            Err(e) => mcp::McpResponse::error(0, -32700, &format!("Parse error: {e}")),
        };

        if let Ok(json) = mcp::serialize_response(&response) {
            let _ = writeln!(out, "{json}");
            let _ = out.flush();
        }
    }
}
