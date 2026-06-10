//! Multiplayer turn-based battle template — two players, each on one end
//! of a [`LoopbackTransport`], submit their commands and the *host* peer
//! advances a single `TurnBattleRunner`. Demonstrates Phase δ's network
//! transport bridging into Phase 1's battle runner.
//!
//! The same architecture works over any [`alice_game_engine::bridge::NetworkTransport`]
//! — swap `LoopbackTransport::pair` for an ALICE-Sync transport in
//! production.

use alice_game_engine::ability::{Attribute, AttributeSet};
use alice_game_engine::battle::{
    BattleAction, BattleCommand, BattleResult, Battler, GridCell, Party, RandomAi, Team,
    TurnBattleRunner,
};
use alice_game_engine::bridge::NetworkTransport;
use alice_game_engine::network::LoopbackTransport;

/// Wire protocol: a single byte action followed by a target byte for
/// Attack, or empty for Defend.
const MSG_ATTACK: u8 = 0x01;
const MSG_DEFEND: u8 = 0x02;

fn encode(cmd: &BattleCommand) -> Vec<u8> {
    match &cmd.action {
        BattleAction::Attack { target_idx } => vec![MSG_ATTACK, *target_idx as u8],
        BattleAction::Defend => vec![MSG_DEFEND],
        _ => vec![MSG_DEFEND], // demo only handles Attack/Defend
    }
}

fn decode(actor_idx: usize, data: &[u8]) -> BattleCommand {
    match data.first().copied() {
        Some(MSG_ATTACK) => BattleCommand {
            actor_idx,
            action: BattleAction::Attack {
                target_idx: data.get(1).copied().unwrap_or(0) as usize,
            },
        },
        _ => BattleCommand {
            actor_idx,
            action: BattleAction::Defend,
        },
    }
}

fn battler(name: &str, hp: f32, atk: f32, speed: f32, team: Team) -> Battler {
    let mut a = AttributeSet::new();
    a.add(Attribute::new("hp", hp, 0.0, hp));
    a.add(Attribute::new("atk", atk, 0.0, 999.0));
    a.add(Attribute::new("def", 0.0, 0.0, 999.0));
    a.add(Attribute::new("speed", speed, 0.0, 999.0));
    Battler::new(name, a, team).with_cell(GridCell::default())
}

fn main() {
    // Set up two peers: 1 = host (runs the runner), 2 = client (sends inputs).
    let (mut host_link, mut client_link) = LoopbackTransport::pair(1, 2);

    // The host owns the battle state.
    let allies = Party::new(vec![battler("Hero", 60.0, 12.0, 10.0, Team::Ally)]);
    let enemies = Party::new(vec![battler("Specter", 50.0, 10.0, 9.0, Team::Enemy)]);
    let mut runner = TurnBattleRunner::new(allies, enemies);
    let mut ai = RandomAi::new(0xC0FFEE);

    println!("=== Multiplayer Battle (LoopbackTransport demo) ===");
    let mut turn = 0;
    while runner.result() == BattleResult::Ongoing {
        turn += 1;
        println!("\n── Turn {turn} ──");

        // Client (peer 2) decides what to do — for the demo, always Attack.
        let client_cmd = BattleCommand {
            actor_idx: 0,
            action: BattleAction::Attack { target_idx: 0 },
        };
        client_link
            .send_to(1, &encode(&client_cmd))
            .expect("client → host");

        // Host (peer 1) receives the client's command and runs the turn.
        let mut commands = Vec::new();
        for (from_peer, bytes) in host_link.recv() {
            println!(
                "  host received {} bytes from peer {from_peer}",
                bytes.len()
            );
            commands.push(decode(0, &bytes));
        }
        let result = runner.run_turn(commands, &mut ai);

        // Replay log to both peers (host streams it to client).
        let log_for_client: Vec<u8> = runner.log().last().map_or(Vec::new(), |s| {
            let mut v = vec![0xFE];
            v.extend_from_slice(s.as_bytes());
            v
        });
        if !log_for_client.is_empty() {
            host_link
                .send_to(2, &log_for_client)
                .expect("host → client");
        }
        for (from_peer, bytes) in client_link.recv() {
            if let Some(0xFE) = bytes.first().copied() {
                let line = std::str::from_utf8(&bytes[1..]).unwrap_or("(non-utf8 log)");
                println!("  client received log from peer {from_peer}: {line}");
            }
        }

        if result != BattleResult::Ongoing {
            println!("\n=== Battle result: {result:?} ===");
            break;
        }
    }

    println!(
        "\nFinal Hero HP: {} | Specter HP: {}",
        runner.allies.battlers[0].attrs.value("hp").max(0.0),
        runner.enemies.battlers[0].attrs.value("hp").max(0.0)
    );
}
