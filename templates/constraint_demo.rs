//! Constraint demo — three scenes showing the new joint kinds:
//!
//! 1. **Slider (piston)** — body B sits offset from the X axis with a
//!    big perpendicular displacement. After solving it snaps onto the
//!    axis line and stays within the offset clamp.
//! 2. **Fixed (weld)** — two bodies are glued at a fixed relative
//!    offset; an external pull on one body does not separate them.
//! 3. **Cone twist (humanoid hip)** — body B starts 80° away from a
//!    vertical twist axis; a 30° cone limit pulls it onto the cone
//!    surface.
//!
//! ```bash
//! cargo run --example constraint_demo
//! ```
//!
//! Headless: just prints positions before and after solving.

use alice_game_engine::joint::{solve_joints, Joint};
use alice_game_engine::math::Vec3;
use alice_game_engine::physics3d::{PhysicsWorld, RigidBody};

fn print_body(label: &str, world: &PhysicsWorld, idx: usize) {
    let p = world.bodies[idx].position;
    println!(
        "  {label:>18}: ({:>7.3}, {:>7.3}, {:>7.3})",
        p.x(),
        p.y(),
        p.z(),
    );
}

fn scene_slider() {
    println!("\n=== Scene 1: Slider (piston) ===");
    let mut world = PhysicsWorld::new();
    world.gravity = Vec3::ZERO;
    let anchor = world.add_body(RigidBody::new_static(Vec3::ZERO));
    let piston = world.add_body(RigidBody::new(Vec3::new(5.0, 2.0, -1.0), 1.0));

    print_body("piston (before)", &world, piston);
    let joint = Joint::slider(anchor, piston, Vec3::X, 0.0, 3.0);
    solve_joints(&mut world, &[joint], 30);
    print_body("piston (after)", &world, piston);
    println!("  axis clamp: X ∈ [0, 3], Y/Z → 0");
}

fn scene_fixed() {
    println!("\n=== Scene 2: Fixed (weld) ===");
    let mut world = PhysicsWorld::new();
    world.gravity = Vec3::ZERO;
    let chassis = world.add_body(RigidBody::new_static(Vec3::ZERO));
    let panel = world.add_body(RigidBody::new(Vec3::new(2.5, 1.5, 0.0), 1.0));

    print_body("panel (before)", &world, panel);
    let offset = Vec3::new(1.0, 0.0, 0.0);
    let joint = Joint::fixed(chassis, panel, offset);
    solve_joints(&mut world, &[joint], 30);
    print_body("panel (after)", &world, panel);
    println!("  weld: panel - chassis == (1, 0, 0)");
}

fn scene_cone_twist() {
    println!("\n=== Scene 3: Cone twist (humanoid hip) ===");
    let mut world = PhysicsWorld::new();
    world.gravity = Vec3::ZERO;
    let pelvis = world.add_body(RigidBody::new_static(Vec3::ZERO));
    let start_angle = 80.0_f32.to_radians();
    let leg_start = Vec3::new(start_angle.sin(), start_angle.cos(), 0.0);
    let thigh = world.add_body(RigidBody::new(leg_start, 1.0));

    print_body("thigh (before)", &world, thigh);
    let cone_limit = 30.0_f32.to_radians();
    let joint = Joint::cone_twist(pelvis, thigh, Vec3::Y, cone_limit, 0.5);
    solve_joints(&mut world, &[joint], 50);
    print_body("thigh (after)", &world, thigh);

    let dir = world.bodies[thigh].position.normalize();
    let cos_angle = dir.dot(Vec3::Y).clamp(-1.0, 1.0);
    let resulting_angle = cos_angle.acos().to_degrees();
    println!("  cone limit: 30°, resulting angle from twist axis: {resulting_angle:.2}°",);
}

fn main() {
    println!("=== Constraint Demo ===");
    scene_slider();
    scene_fixed();
    scene_cone_twist();
}
