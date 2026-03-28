//! Hello Engine: demonstrates window creation, scene graph with hybrid
//! mesh + SDF nodes, animation, and the deferred render pipeline.
//!
//! Run: `cargo run --example hello_engine --features full`

use alice_game_engine::animation::{AnimationClip, AnimationPlayer, Keyframe, Track};
use alice_game_engine::engine::{Engine, EngineConfig, EngineContext, System};
use alice_game_engine::math::{Color, Quat, Vec3};
use alice_game_engine::scene_graph::{
    CameraData, LightData, LightVariant, LocalTransform, MeshData, Node, NodeKind, SdfData,
};
use alice_game_engine::window::{FrameTimer, WindowConfig};

struct HelloSystem {
    cube_node: Option<alice_game_engine::scene_graph::NodeId>,
    player: AnimationPlayer,
    clip: AnimationClip,
}

impl HelloSystem {
    fn new() -> Self {
        // Create a rotation animation
        let mut clip = AnimationClip::new("spin");
        clip.looping = true;
        let mut track = Track::new("rotation_y");
        track.add_keyframe(Keyframe::new(0.0, 0.0));
        track.add_keyframe(Keyframe::new(4.0, std::f32::consts::TAU));
        clip.tracks.push(track);

        Self {
            cube_node: None,
            player: AnimationPlayer::new("spin"),
            clip,
        }
    }
}

impl System for HelloSystem {
    fn init(&mut self, ctx: &mut EngineContext) {
        // Camera
        ctx.scene
            .add(Node::new("camera", NodeKind::Camera(CameraData::default())));

        // Directional light
        ctx.scene.add(Node::new(
            "sun",
            NodeKind::Light(LightData {
                variant: LightVariant::Directional,
                color: Color::WHITE,
                intensity: 1.0,
                cast_shadows: true,
            }),
        ));

        // Mesh cube
        let mut cube = Node::new("cube", NodeKind::Mesh(MeshData::default()));
        cube.local_transform = LocalTransform {
            position: Vec3::new(-2.0, 0.0, -5.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        };
        self.cube_node = Some(ctx.scene.add(cube));

        // SDF sphere
        let mut sphere = Node::new(
            "sdf_sphere",
            NodeKind::Sdf(SdfData {
                sdf_json: r#"{"Primitive":{"Sphere":{"radius":1.0}}}"#.to_string(),
                half_extents: Vec3::new(1.0, 1.0, 1.0),
                generate_collider: false,
            }),
        );
        sphere.local_transform.position = Vec3::new(2.0, 0.0, -5.0);
        ctx.scene.add(sphere);

        // Start animation
        self.player.play();

        println!("Scene initialized:");
        println!("  - {} nodes", ctx.scene.node_count());
        println!("  - {} meshes", ctx.scene.meshes().len());
        println!("  - {} SDF volumes", ctx.scene.sdf_volumes().len());
        println!("  - {} lights", ctx.scene.lights().len());
        println!("  - {} cameras", ctx.scene.cameras().len());
    }

    fn update(&mut self, ctx: &mut EngineContext, dt: f32) {
        // Advance animation
        self.player.update(dt);
        let values = self.clip.evaluate(self.player.time);

        // Apply rotation to cube
        if let Some(cube_id) = self.cube_node {
            if let Some(("rotation_y", angle)) = values.first().copied() {
                if let Some(node) = ctx.scene.get_mut(cube_id) {
                    node.local_transform.rotation = Quat::from_axis_angle(Vec3::Y, angle);
                }
            }
        }
    }
}

fn main() {
    let _window_config = WindowConfig {
        title: "ALICE Engine — Hello World".to_string(),
        width: 1280,
        height: 720,
        ..WindowConfig::default()
    };

    let engine_config = EngineConfig {
        window_title: "ALICE Engine — Hello World".to_string(),
        window_width: 1280,
        window_height: 720,
        ..EngineConfig::default()
    };

    let mut engine = Engine::new(engine_config);
    let mut system = HelloSystem::new();
    engine.init(&mut system);

    // Simulate 300 frames (in a real app this would be driven by winit event loop)
    let mut timer = FrameTimer::new();
    for frame in 0..300 {
        let time_ms = frame as f64 * 16.666;
        timer.update(time_ms);
        engine.frame(timer.delta_seconds, &mut system);
    }

    println!(
        "Ran {} frames, FPS: {:.1}",
        engine.frame_count(),
        timer.smoothed_fps()
    );
    println!(
        "Cube rotation: {:.2} rad",
        system.player.time % std::f32::consts::TAU
    );
}
