//! Humanoid + Expression demo — binds a VRM-style humanoid skeleton
//! and drives a lip-sync via expression channels.
//!
//! ```bash
//! cargo run --example humanoid_demo
//! ```

use alice_game_engine::humanoid::{ExpressionChannel, ExpressionSet, Humanoid, HumanoidBone};

fn main() {
    println!("=== Humanoid + Expression Demo ===");

    // Bind a minimal VRM humanoid (just give every required bone a fake index).
    let mut h = Humanoid::new();
    for (i, bone) in HumanoidBone::required().iter().enumerate() {
        h.bind(*bone, i as u32);
    }
    h.bind(HumanoidBone::head, 100);
    h.bind(HumanoidBone::jaw, 101);

    println!("bound bones: {}", h.bound_count());
    println!("meets VRM required minimum? {}", h.meets_required());
    println!("missing required: {:?}", h.missing_required());
    println!("head bone index: {:?}", h.get(HumanoidBone::head));

    // Lip-sync over "konnichi-wa" (簡略化: 3 viseme frames).
    let mut e = ExpressionSet::new();
    let frames = [
        (
            "ko (Ou)",
            ExpressionChannel::Ou,
            ExpressionChannel::Oh,
            0.8,
            0.2,
        ),
        (
            "nn (Ih)",
            ExpressionChannel::Ih,
            ExpressionChannel::Ee,
            0.6,
            0.4,
        ),
        (
            "chi (Ih)",
            ExpressionChannel::Ih,
            ExpressionChannel::Aa,
            0.7,
            0.3,
        ),
        (
            "wa (Aa)",
            ExpressionChannel::Aa,
            ExpressionChannel::Oh,
            0.85,
            0.15,
        ),
    ];

    println!("\nlip-sync trace:");
    for (label, primary, secondary, pw, sw) in &frames {
        e.reset();
        e.set_visemes(primary.clone(), *pw, secondary.clone(), *sw);
        let pv = e.weight(primary);
        let sv = e.weight(secondary);
        println!(
            "  {label}: primary={:?}={pv:.2}, secondary={:?}={sv:.2}",
            primary, secondary,
        );
    }

    // Blink + happy expression overlay.
    e.set(ExpressionChannel::BlinkLeft, 1.0);
    e.set(ExpressionChannel::BlinkRight, 1.0);
    e.set(ExpressionChannel::Happy, 0.5);
    println!("\nblink + happy:");
    for (ch, w) in e.iter() {
        println!("  {ch:?}: {w:.2}");
    }
    println!("\ncustom channel 'smirk':");
    let smirk = ExpressionChannel::Custom("smirk".into());
    e.set(smirk.clone(), 0.65);
    println!("  weight = {:.2}", e.weight(&smirk));
}
