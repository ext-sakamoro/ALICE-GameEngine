# ALICE-GameEngine

Game loop and ECS — entity-component-system integrated in SDF space.

## Features

- **Entity-Component-System**: Sparse-set based ECS with generation-tracked entity IDs
- **Physics System**: AABB collision detection and velocity-based movement
- **Scene Management**: Named scenes with entity grouping
- **Input Handling**: Keyboard input state tracking
- **Game Time**: Delta time, total time, and frame counting
- **Zero Dependencies**: Pure Rust, no external crates

## Usage

```rust
use alice_game_engine::{World, GameTime, PhysicsSystem, Transform, Velocity};

let mut world = World::new();
let entity = world.spawn();

world.transform_store.insert(entity, Transform::new(0.0, 0.0));
world.velocity_store.insert(entity, Velocity::new(1.0, 0.5));

let mut time = GameTime::new();
time.tick(1.0 / 60.0);

PhysicsSystem::update(&mut world, &time);
```

## License

MIT
