//! Deferred decal demo — projects five decals onto a wall mesh and a floor
//! SDF, then prints the per-decal AABB and frame draw statistics. Headless
//! (no window), so it runs anywhere `cargo run --example decal_demo` is
//! invoked.
//!
//! ```bash
//! cargo run --example decal_demo
//! ```

use alice_game_engine::decal::{DecalBlendMode, DecalData};
use alice_game_engine::math::{Color, Quat, Vec3};
use alice_game_engine::render_pipeline::FrameData;
use alice_game_engine::scene_graph::{
    CameraData, LightData, MeshData, Node, NodeKind, SceneGraph, SdfData,
};

struct Placement {
    name: &'static str,
    position: Vec3,
    rotation: Quat,
    half_extents: Vec3,
    color: Color,
    opacity: f32,
    blend: DecalBlendMode,
}

fn main() {
    println!("=== Deferred Decal Demo ===");

    let mut scene = SceneGraph::new("decal_demo");

    // Camera looking at the wall.
    let cam_id = scene.add(Node::new("cam", NodeKind::Camera(CameraData::default())));
    if let Some(cam) = scene.get_mut(cam_id) {
        cam.local_transform.position = Vec3::new(0.0, 1.5, -8.0);
    }

    // Wall (mesh) at z = 0, facing the camera.
    let wall_id = scene.add(Node::new(
        "wall",
        NodeKind::Mesh(MeshData {
            mesh_id: 1,
            material_id: 0,
            cast_shadows: true,
        }),
    ));
    if let Some(wall) = scene.get_mut(wall_id) {
        wall.local_transform.scale = Vec3::new(8.0, 4.0, 0.2);
    }

    // Floor (SDF box) at y = 0.
    let floor_id = scene.add(Node::new(
        "floor",
        NodeKind::Sdf(SdfData {
            sdf_json: r#"{"kind":"box","half_extents":[10,0.1,10]}"#.into(),
            half_extents: Vec3::new(10.0, 0.1, 10.0),
            generate_collider: true,
        }),
    ));
    if let Some(floor) = scene.get_mut(floor_id) {
        floor.local_transform.position = Vec3::new(0.0, -0.1, 0.0);
    }

    // Key light.
    scene.add(Node::new("sun", NodeKind::Light(LightData::default())));

    // Five decals: bullet hole, blood, graffiti, sign, glowing logo.
    let placements = [
        Placement {
            name: "bullet_hole",
            position: Vec3::new(-2.0, 2.0, 0.05),
            rotation: Quat::IDENTITY,
            half_extents: Vec3::new(0.25, 0.25, 0.3),
            color: Color::new(0.1, 0.1, 0.1, 1.0),
            opacity: 0.9,
            blend: DecalBlendMode::AlphaBlend,
        },
        Placement {
            name: "blood_splatter",
            position: Vec3::new(-0.5, 1.5, 0.05),
            rotation: Quat::IDENTITY,
            half_extents: Vec3::new(0.6, 0.6, 0.3),
            color: Color::new(0.7, 0.05, 0.05, 1.0),
            opacity: 0.85,
            blend: DecalBlendMode::Multiply,
        },
        Placement {
            name: "graffiti_tag",
            position: Vec3::new(1.5, 2.2, 0.05),
            rotation: Quat::IDENTITY,
            half_extents: Vec3::new(0.8, 0.4, 0.3),
            color: Color::new(0.2, 0.9, 0.6, 1.0),
            opacity: 0.95,
            blend: DecalBlendMode::AlphaBlend,
        },
        Placement {
            name: "warning_sign",
            position: Vec3::new(2.8, 1.0, 0.05),
            rotation: Quat::from_axis_angle(Vec3::Z, std::f32::consts::FRAC_PI_8),
            half_extents: Vec3::new(0.5, 0.5, 0.3),
            color: Color::new(1.0, 0.7, 0.0, 1.0),
            opacity: 1.0,
            blend: DecalBlendMode::AlphaBlend,
        },
        Placement {
            name: "glowing_rune",
            position: Vec3::new(0.0, 3.0, 0.05),
            rotation: Quat::IDENTITY,
            half_extents: Vec3::new(0.4, 0.4, 0.3),
            color: Color::new(0.4, 0.6, 1.0, 1.0),
            opacity: 1.0,
            blend: DecalBlendMode::Additive,
        },
    ];

    for p in &placements {
        let mut node = Node::new(
            p.name,
            NodeKind::Decal(DecalData {
                albedo_texture: None,
                normal_texture: None,
                color: p.color,
                opacity: p.opacity,
                layer_mask: u32::MAX,
                blend_mode: p.blend,
            }),
        );
        node.local_transform.position = p.position;
        node.local_transform.rotation = p.rotation;
        node.local_transform.scale = p.half_extents;
        scene.add(node);
    }

    scene.update_world_matrices();

    let frame = FrameData::from_scene(&scene).expect("scene has a camera");
    println!(
        "Frame: mesh_draws={}, sdf_draws={}, decal_draws={}, lights={}",
        frame.mesh_draws.len(),
        frame.sdf_draws.len(),
        frame.decal_draws.len(),
        frame.light_count,
    );

    for (draw, place) in frame.decal_draws.iter().zip(placements.iter()) {
        let (min, max) = draw.world_aabb();
        println!(
            "  {:>16}  blend={:?}  opacity={:.2}  aabb=[{:.2},{:.2},{:.2}]..[{:.2},{:.2},{:.2}]",
            place.name,
            draw.data.blend_mode,
            draw.data.opacity,
            min.x(),
            min.y(),
            min.z(),
            max.x(),
            max.y(),
            max.z(),
        );
    }

    println!(
        "\nTotal draw count this frame: {}",
        frame.total_draw_count()
    );
}
