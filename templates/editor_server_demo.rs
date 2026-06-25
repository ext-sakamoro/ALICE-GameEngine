//! Editor websocket server demo — runs an axum server on
//! `127.0.0.1:8088` that exposes the [`dispatch_client_message`]
//! protocol over a websocket on `/ws`. A browser editor (or any
//! `wscat`-style client) sends JSON-encoded [`EditorClientMessage`]
//! frames and the server replies with [`EditorServerMessage`].
//!
//! ```bash
//! cargo run --example editor_server_demo --features editor_server
//! # then in another shell:
//! # echo '{"kind":"hello","protocol_version":1}' | websocat ws://127.0.0.1:8088/ws
//! ```
//!
//! The shared scene + editor live behind a `tokio::sync::Mutex` so
//! multiple clients see consistent state.

use std::net::SocketAddr;
use std::sync::Arc;

use alice_game_engine::editor::{
    dispatch_client_message, Editor, EditorClientMessage, EditorServerMessage,
};
use alice_game_engine::scene_graph::{CameraData, Node, NodeKind, SceneGraph};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use tokio::sync::Mutex;

#[derive(Clone)]
struct AppState {
    editor: Arc<Mutex<Editor>>,
    scene: Arc<Mutex<SceneGraph>>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut scene = SceneGraph::new("editor-server-demo");
    scene.add(Node::new("cam", NodeKind::Camera(CameraData::default())));

    let state = AppState {
        editor: Arc::new(Mutex::new(Editor::new())),
        scene: Arc::new(Mutex::new(scene)),
    };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/", get(root))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 8088));
    println!("editor_server listening on http://{addr}/  (ws://{addr}/ws)");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn root() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("editor_ui/index.html"))
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    while let Some(Ok(msg)) = socket.recv().await {
        if let Message::Text(text) = msg {
            let reply = match serde_json::from_str::<EditorClientMessage>(&text) {
                Ok(client_msg) => {
                    let mut editor = state.editor.lock().await;
                    let mut scene = state.scene.lock().await;
                    dispatch_client_message(&mut editor, &mut scene, client_msg)
                }
                Err(e) => EditorServerMessage::Error {
                    message: format!("invalid JSON: {e}"),
                },
            };
            let reply_text = serde_json::to_string(&reply).unwrap_or_else(|e| {
                format!(r#"{{"kind":"error","message":"serialize failed: {e}"}}"#)
            });
            if socket.send(Message::Text(reply_text.into())).await.is_err() {
                break;
            }
        } else if matches!(msg, Message::Close(_)) {
            break;
        }
    }
}
