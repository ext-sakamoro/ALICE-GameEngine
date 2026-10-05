//! Constraint demo — three scenes showing joint kinds solved by
//! `alice_physics`:
//!
//! 1. **Slider (piston)** — body B starts off the X axis; the slider
//!    pulls it onto the axis line and keeps it within the offset clamp.
//! 2. **Fixed (weld)** — a panel welded to a static chassis holds its
//!    offset under gravity.
//! 3. **Cone twist (humanoid hip)** — a leg hanging below the pelvis is
//!    kicked sideways; the 30° swing limit stops it.
//!
//! ```bash
//! cargo run --example constraint_demo --features physics
//! ```
//!
//! Headless: just prints positions.

use alice_game_engine::joint::Joint;
use alice_game_engine::math::Vec3;
use alice_game_engine::physics3d::{BodyDesc, BodyHandle, PhysicsConfig, PhysicsWorld};

fn print_body(label: &str, world: &PhysicsWorld, body: BodyHandle) {
    let p = world.position(body).unwrap_or_default();
    println!("  {label:>18}: ({:>7.3}, {:>7.3}, {:>7.3})", p.x, p.y, p.z);
}

fn world_with_gravity(gravity: glam::Vec3) -> PhysicsWorld {
    PhysicsWorld::new(PhysicsConfig {
        gravity,
        ..PhysicsConfig::default()
    })
}

fn run(world: &mut PhysicsWorld, seconds: f32) {
    let steps = (seconds * 60.0) as u32;
    for _ in 0..steps {
        world.step_fixed();
    }
}

fn scene_slider() {
    println!("\n=== Scene 1: Slider (piston) ===");
    let mut world = world_with_gravity(glam::Vec3::ZERO);
    let anchor = world.add_body(BodyDesc::fixed(glam::Vec3::ZERO, 0.0));
    let piston = world.add_body(BodyDesc::dynamic(glam::Vec3::new(5.0, 2.0, -1.0), 1.0, 0.0));

    print_body("piston (before)", &world, piston);
    world.add_joint(Joint::slider(anchor, piston, Vec3::X, 0.0, 3.0));
    run(&mut world, 1.0);
    print_body("piston (after)", &world, piston);
    println!("  axis clamp: X ∈ [0, 3], Y/Z → 0");
}

fn scene_fixed() {
    println!("\n=== Scene 2: Fixed (weld) ===");
    let mut world = world_with_gravity(glam::Vec3::new(0.0, -9.81, 0.0));
    let chassis = world.add_body(BodyDesc::fixed(glam::Vec3::ZERO, 0.0));
    let panel = world.add_body(BodyDesc::dynamic(glam::Vec3::new(1.0, 0.0, 0.0), 1.0, 0.0));

    world.add_joint(Joint::fixed(chassis, panel, Vec3::new(1.0, 0.0, 0.0)));
    run(&mut world, 2.0);
    print_body("panel (after 2 s)", &world, panel);
    println!("  weld: panel - chassis == (1, 0, 0) under gravity");
}

fn scene_cone_twist() {
    println!("\n=== Scene 3: Cone twist (humanoid hip) ===");
    let mut world = world_with_gravity(glam::Vec3::ZERO);
    let pelvis = world.add_body(BodyDesc::fixed(glam::Vec3::ZERO, 0.0));
    let thigh = world.add_body(BodyDesc::dynamic(glam::Vec3::new(0.0, -1.0, 0.0), 1.0, 0.0));

    let cone_limit = 30.0_f32.to_radians();
    world.add_joint(Joint::cone_twist(
        pelvis,
        thigh,
        Vec3::new(0.0, -1.0, 0.0),
        cone_limit,
        0.5,
    ));
    world.set_velocity(thigh, glam::Vec3::new(4.0, 0.0, 0.0));
    let mut widest = 0.0_f32;
    for _ in 0..120 {
        world.step_fixed();
        let dir = world
            .position(thigh)
            .unwrap_or_default()
            .normalize_or_zero();
        widest = widest.max(dir.dot(glam::Vec3::NEG_Y).clamp(-1.0, 1.0).acos());
    }
    print_body("thigh (after)", &world, thigh);
    println!(
        "  cone limit: 30°, widest swing: {:.2}°",
        widest.to_degrees()
    );
}

fn main() {
    println!("=== Constraint Demo ===");
    scene_slider();
    scene_fixed();
    scene_cone_twist();
}
