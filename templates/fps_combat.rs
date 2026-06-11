//! FPS shooter template — `LockOn` + ranged hitscan + per-target invuln
//! using the `action_combat` module. Demonstrates the targeting and damage
//! pipeline without depending on a render loop.
//!
//! ```bash
//! cargo run --example fps_combat
//! ```

use alice_game_engine::action_combat::{
    resolve_hits, ColliderShape, HitStop, Hitbox, Hurtbox, LockOn, LockOnCandidate,
};
use alice_game_engine::math::Vec3;

struct Enemy {
    id: u32,
    hp: i32,
    pos: Vec3,
}

fn main() {
    println!("=== FPS Combat Demo ===");

    let viewer = Vec3::new(0.0, 1.6, 0.0);
    let forward = Vec3::new(0.0, 0.0, 1.0);

    let mut enemies = vec![
        Enemy {
            id: 10,
            hp: 30,
            pos: Vec3::new(0.5, 1.6, 6.0),
        }, // ahead
        Enemy {
            id: 11,
            hp: 30,
            pos: Vec3::new(-3.0, 1.6, 4.0),
        }, // off to the left
        Enemy {
            id: 12,
            hp: 30,
            pos: Vec3::new(0.0, 1.6, -3.0),
        }, // behind
    ];

    let mut lock = LockOn::new(20.0, 0.6);
    let mut hit_stop = HitStop::default();

    for shot in 0..6 {
        // Pick a target each frame.
        let candidates: Vec<LockOnCandidate> = enemies
            .iter()
            .filter(|e| e.hp > 0)
            .map(|e| LockOnCandidate {
                entity: e.id,
                position: e.pos,
            })
            .collect();
        let target_id = lock.acquire(viewer, forward, &candidates);

        // Fire a hitscan (instant Hitbox).
        let Some(target_id) = target_id else {
            println!("shot {shot}: no lock");
            continue;
        };
        let target_pos = enemies
            .iter()
            .find(|e| e.id == target_id)
            .map(|e| e.pos)
            .unwrap();

        let mut hits = vec![{
            let mut hb = Hitbox::new(
                shot,
                u32::MAX, // shooter id (placeholder)
                ColliderShape::Sphere {
                    center: target_pos,
                    radius: 0.4,
                },
                "rifle_hitscan",
            );
            hb.damage = 12.0;
            hb.hitstop_frames = 2;
            hb
        }];
        let mut hurts: Vec<_> = enemies
            .iter()
            .filter(|e| e.hp > 0)
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
                if e.hp <= 0 {
                    println!("shot {shot}: locked {} → KILL ({} dmg)", ev.target, dmg);
                } else {
                    println!(
                        "shot {shot}: locked {} → {} dmg ({} HP left)",
                        ev.target, dmg, e.hp
                    );
                }
            }
            hit_stop.trigger(ev.hitstop_frames);
        }
    }

    let alive = enemies.iter().filter(|e| e.hp > 0).count();
    println!(
        "\n=== Final ===\nEnemies remaining: {alive} / {}",
        enemies.len()
    );
    println!("Lock current   : {:?}", lock.current);
}
