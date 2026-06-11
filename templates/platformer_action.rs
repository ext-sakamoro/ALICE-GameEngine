//! 2D platformer with action-combat hitbox + curl-noise dash trail. Shows
//! how `action_combat` and `particle::TrailEmitter` compose into a snappy
//! action loop on top of `scene2d`.
//!
//! ```bash
//! cargo run --example platformer_action
//! ```

use alice_game_engine::action_combat::{resolve_hits, ColliderShape, HitStop, Hitbox, Hurtbox};
use alice_game_engine::math::Vec3;
use alice_game_engine::particle::TrailEmitter;

const PLAYER: u32 = 1;
const ENEMY_A: u32 = 100;
const ENEMY_B: u32 = 101;

struct Combatant {
    id: u32,
    hp: i32,
    pos: Vec3,
}

fn main() {
    println!("=== Platformer Action Demo ===");

    // Three combatants: player + 2 enemies.
    let mut player = Combatant {
        id: PLAYER,
        hp: 40,
        pos: Vec3::new(0.0, 1.0, 0.0),
    };
    let mut enemies = vec![
        Combatant {
            id: ENEMY_A,
            hp: 14,
            pos: Vec3::new(1.4, 1.0, 0.0),
        },
        Combatant {
            id: ENEMY_B,
            hp: 14,
            pos: Vec3::new(4.0, 1.0, 0.0),
        },
    ];

    let mut hit_stop = HitStop::default();
    let mut dash_trail = TrailEmitter::new(player.pos);
    dash_trail.spawn_interval = 0.02;
    dash_trail.life_per_segment = 0.4;

    // Simulate 30 frames at 60 Hz.
    for frame in 0..30 {
        let dt = 1.0 / 60.0;
        let t = frame as f32 * dt;

        // Player dashes right for the first 0.3 s.
        if t < 0.3 {
            player.pos = Vec3::new(player.pos.x() + 8.0 * dt, player.pos.y(), 0.0);
        }
        // Update visual trail.
        dash_trail.source = player.pos;
        dash_trail.update(dt);

        // Time freeze while a hit-stop is in flight.
        if hit_stop.is_active() {
            hit_stop.step();
            println!(
                "frame {frame:>2} | HITSTOP (remaining {})",
                hit_stop.remaining_frames + 1
            );
            continue;
        }

        // Swing the sword every 8th frame.
        if frame % 8 == 7 {
            let mut hits = vec![{
                let mut hb = Hitbox::new(
                    frame as u32,
                    player.id,
                    ColliderShape::Sphere {
                        center: Vec3::new(player.pos.x() + 0.8, player.pos.y(), 0.0),
                        radius: 1.0,
                    },
                    "sword_slash",
                );
                hb.damage = 8.0;
                hb.hitstop_frames = 3;
                hb
            }];
            let mut hurts: Vec<_> = enemies
                .iter()
                .map(|e| {
                    Hurtbox::new(
                        e.id,
                        e.id,
                        ColliderShape::Sphere {
                            center: e.pos,
                            radius: 0.5,
                        },
                    )
                })
                .collect();
            let events = resolve_hits(&mut hits, &mut hurts);

            for ev in &events {
                let dmg = ev.damage.round() as i32;
                if let Some(e) = enemies.iter_mut().find(|e| e.id == ev.target) {
                    e.hp -= dmg;
                }
                hit_stop.trigger(ev.hitstop_frames);
                println!(
                    "frame {frame:>2} | HIT  player → {} for {} dmg (source={})",
                    ev.target, dmg, ev.source
                );
            }
            if events.is_empty() {
                println!(
                    "frame {frame:>2} | swing (no contact, player at x={:.1})",
                    player.pos.x()
                );
            }
        }
    }

    enemies.retain(|e| e.hp > 0);
    println!("\n=== Final state ===");
    println!("  Player HP : {} | x = {:.1}", player.hp, player.pos.x());
    println!("  Enemies left : {}", enemies.len());
    println!("  Trail segments left : {}", dash_trail.len());
}
