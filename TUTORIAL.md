# ALICE-GameEngine — Getting Started

This is the step-by-step tutorial for first-time users. It covers
the five most commonly-needed flows; the API surface beyond these
is documented per-module in `LLM_REFERENCE.md` + rustdoc.

---

## 1. Install + build a one-screen game

Add the engine to your `Cargo.toml`:

```toml
[dependencies]
alice-game-engine = { version = "0.6", features = ["full"] }
```

Write a five-line headless game:

```rust
use alice_game_engine::easy::*;

fn main() {
    let mut game = GameBuilder::new("Demo").build();
    game.add_camera();
    game.add_cube(0.0, 1.0, -5.0);
    game.add_sphere_sdf(3.0, 0.0, 0.0, 1.0);
    game.add_light(0.0, 10.0, 0.0);
    game.run_headless(300);
}
```

Run it:

```bash
cargo run
```

For the GPU windowed version, swap `run_headless` for
`run_windowed` and add `--features window`.

---

## 2. Spawn an NPC that talks to an LLM

```rust
use alice_game_engine::llm::*;

let llm = MockLlm::new("Welcome, traveler.");
let mut npc = NpcContext::new("Guard", "a stern guard");
let reply = npc.respond("Hello!", &llm).unwrap();
println!("{}: {}", npc.name(), reply);
```

Real Claude / OpenAI / local-llama integration: implement
`LlmProvider` for your client and pass it where `MockLlm` goes.

---

## 3. Build a scene with mixed SDF + mesh geometry

```rust
use alice_game_engine::scene_graph::*;
use alice_game_engine::math::Vec3;

let mut scene = SceneGraph::new("level");
let cam = scene.add(Node::new("cam", NodeKind::Camera(CameraData::default())));
let cube = scene.add(Node::new("hero", NodeKind::Mesh(MeshData::default())));
let sphere = scene.add(Node::new(
    "blob",
    NodeKind::Sdf(SdfData {
        sdf_json: r#"{"kind":"sphere","radius":1.0}"#.into(),
        half_extents: Vec3::new(1.0, 1.0, 1.0),
        generate_collider: true,
    }),
));
scene.update_world_matrices();
```

Add a decal, env probe, or any of the other 60+ node kinds the
same way. See `LLM_REFERENCE.md` for the full `NodeKind` table.

---

## 4. Run a turn-based RPG battle

```rust
use alice_game_engine::ability::{Attribute, AttributeSet};
use alice_game_engine::battle::*;

fn battler(name: &str, hp: f32, atk: f32, team: Team) -> Battler {
    let mut a = AttributeSet::new();
    a.add(Attribute::new("hp", hp, 0.0, hp));
    a.add(Attribute::new("atk", atk, 0.0, 999.0));
    a.add(Attribute::new("def", 0.0, 0.0, 999.0));
    a.add(Attribute::new("speed", 10.0, 0.0, 999.0));
    Battler::new(name, a, team)
}

let allies = Party::new(vec![battler("Hero", 60.0, 12.0, Team::Ally)]);
let enemies = Party::new(vec![battler("Slime", 25.0, 4.0, Team::Enemy)]);
let mut runner = TurnBattleRunner::new(allies, enemies);
let mut ai = RandomAi::new(1);

while runner.result() == BattleResult::Ongoing {
    let cmd = BattleCommand {
        actor_idx: 0,
        action: BattleAction::Attack { target_idx: 0 },
    };
    runner.run_turn(vec![cmd], &mut ai);
}
assert_eq!(runner.result(), BattleResult::Win);
```

---

## 5. Drive the editor from a web browser

Start the websocket server:

```bash
cargo run --example editor_server_demo --features editor_server
# server listens on 127.0.0.1:8088
```

Open `http://127.0.0.1:8088/` in a browser and click the buttons —
the embedded HTML talks to `/ws` via the
`EditorClientMessage` / `EditorServerMessage` protocol defined in
`editor.rs`.

Programmatic clients (MCP, CI scripts, REPLs) can use
`dispatch_client_message` directly:

```rust
use alice_game_engine::editor::*;
use alice_game_engine::scene_graph::SceneGraph;

let mut editor = Editor::new();
let mut scene = SceneGraph::new("demo");
let reply = dispatch_client_message(
    &mut editor,
    &mut scene,
    EditorClientMessage::Snapshot,
);
println!("{reply:?}");
```

---

## Where to read next

- `README.md` — top-level "What's in the box" summary
- `LLM_REFERENCE.md` — per-feature one-liner with code template
- `CHANGELOG.md` — release notes (= newest section is v0.7 wave 4)
- `rustdoc` — `cargo doc --no-deps --features full --open`

For deeper topics (SDF construction, physics, GPU compute pipelines,
shader naga validation, mobile build) every module has rustdoc with
runnable examples; start from `alice_game_engine` crate root.
