//! Spinning Cube: opens a window and renders a rotating colored cube
//! using wgpu. Press Escape to exit.
//!
//! Run: `cargo run --example spinning_cube --features full`

use alice_game_engine::app::{run_windowed, AppCallbacks};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::math::{Quat, Vec3};
use alice_game_engine::scene_graph::{
    CameraData, LightData, LightVariant, LocalTransform, MeshData, Node, NodeKind, SdfData,
};
use alice_game_engine::window::WindowConfig;

struct SpinningCubeApp;

impl AppCallbacks for SpinningCubeApp {
    fn init(&mut self, ctx: &mut EngineContext) {
        ctx.scene
            .add(Node::new("camera", NodeKind::Camera(CameraData::default())));
        ctx.scene.add(Node::new(
            "sun",
            NodeKind::Light(LightData {
                variant: LightVariant::Directional,
                ..LightData::default()
            }),
        ));

        let mut cube = Node::new("cube", NodeKind::Mesh(MeshData::default()));
        cube.local_transform = LocalTransform {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        };
        ctx.scene.add(cube);

        let mut sphere = Node::new(
            "sdf_sphere",
            NodeKind::Sdf(SdfData {
                sdf_json: r#"{"Primitive":{"Sphere":{"radius":0.8}}}"#.to_string(),
                half_extents: Vec3::new(0.8, 0.8, 0.8),
                generate_collider: false,
            }),
        );
        sphere.local_transform.position = Vec3::new(2.5, 0.0, 0.0);
        ctx.scene.add(sphere);

        println!("Scene: {} nodes", ctx.scene.node_count());
    }

    fn update(&mut self, ctx: &mut EngineContext, _dt: f32) {
        let t = ctx.time.total_seconds as f32;
        if let Some(node) = ctx.scene.get_mut(alice_game_engine::scene_graph::NodeId(2)) {
            node.local_transform.rotation = Quat::from_axis_angle(Vec3::Y, t);
        }
    }
}

fn main() {
    let window_config = WindowConfig {
        title: "ALICE Engine — Spinning Cube".to_string(),
        width: 1280,
        height: 720,
        ..WindowConfig::default()
    };

    let engine_config = EngineConfig {
        window_title: "ALICE Engine — Spinning Cube".to_string(),
        ..EngineConfig::default()
    };

    if let Err(e) = run_windowed(window_config, engine_config, Box::new(SpinningCubeApp)) {
        eprintln!("Error: {e}");
    }
}
