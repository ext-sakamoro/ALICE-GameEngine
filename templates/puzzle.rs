//! Puzzle template with undo/redo using Verse transactions.
//!
//! Copy this file to start a Sokoban-style puzzle game.

use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::verse::*;

#[derive(Clone, PartialEq)]
struct PuzzleState {
    player: (i32, i32),
    boxes: Vec<(i32, i32)>,
    goals: Vec<(i32, i32)>,
    moves: u32,
}

struct PuzzleGame {
    state: PuzzleState,
    history: Vec<PuzzleState>,
}

impl PuzzleGame {
    fn new() -> Self {
        let state = PuzzleState {
            player: (1, 1),
            boxes: vec![(3, 2), (4, 4)],
            goals: vec![(5, 2), (5, 4)],
            moves: 0,
        };
        Self {
            state,
            history: Vec::new(),
        }
    }

    fn try_move(&mut self, dx: i32, dy: i32) {
        // Save state for undo via Transaction
        let result = Transaction::execute(&mut self.state, |s| {
            let new_pos = (s.player.0 + dx, s.player.1 + dy);

            // Check bounds
            decide(new_pos.0 >= 0 && new_pos.0 < 8 && new_pos.1 >= 0 && new_pos.1 < 8)?;

            // Push box if present
            if let Some(bi) = s.boxes.iter().position(|b| *b == new_pos) {
                let box_new = (new_pos.0 + dx, new_pos.1 + dy);
                decide(box_new.0 >= 0 && box_new.0 < 8 && box_new.1 >= 0 && box_new.1 < 8)?;
                decide(!s.boxes.contains(&box_new))?;
                s.boxes[bi] = box_new;
            }

            s.player = new_pos;
            s.moves += 1;
            Ok(())
        });

        if result.is_ok() {
            self.history.push(self.state.clone());
        }
    }

    fn undo(&mut self) {
        if let Some(prev) = self.history.pop() {
            self.state = prev;
        }
    }

    fn is_solved(&self) -> bool {
        self.state.goals.iter().all(|g| self.state.boxes.contains(g))
    }
}

impl AppCallbacks for PuzzleGame {
    fn init(&mut self, _ctx: &mut EngineContext) {
        println!("Puzzle: push {} boxes to goals. {} moves so far.",
            self.state.boxes.len(), self.state.moves);
    }

    fn update(&mut self, _ctx: &mut EngineContext, _dt: f32) {
        // Auto-solve demo: try moves
        static MOVES: &[(i32, i32)] = &[(1, 0), (1, 0), (0, 1), (1, 0), (0, -1), (1, 0)];
        let idx = self.state.moves as usize;
        if idx < MOVES.len() && !self.is_solved() {
            self.try_move(MOVES[idx].0, MOVES[idx].1);
        }
    }
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = PuzzleGame::new();
    runner.init(&mut game);
    runner.run_frames(300, 60.0, &mut game);
    println!(
        "Player: {:?} | Moves: {} | Solved: {}",
        game.state.player, game.state.moves, game.is_solved()
    );
}
