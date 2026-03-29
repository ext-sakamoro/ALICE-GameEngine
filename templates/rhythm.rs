//! Rhythm Game template (BeatMap style).
//!
//! BPM-synced notes, timing judgment, combo system.

use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::scripting::*;

#[derive(Clone)]
struct Note {
    time: f32,
    lane: u8,
    hit: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Judgment {
    Perfect,
    Great,
    Good,
    Miss,
}

struct RhythmGame {
    bpm: f32,
    notes: Vec<Note>,
    current_time: f32,
    score: u32,
    combo: u32,
    max_combo: u32,
    judgments: Vec<Judgment>,
    timers: TimerManager,
}

impl RhythmGame {
    fn new() -> Self {
        let bpm = 120.0;
        let beat = 60.0 / bpm;

        // Generate a simple beat pattern
        let mut notes = Vec::new();
        for i in 0..16 {
            notes.push(Note {
                time: i as f32 * beat,
                lane: (i % 4) as u8,
                hit: false,
            });
        }

        let mut timers = TimerManager::new();
        timers.add(Timer::new("beat", beat, TimerMode::Repeating));

        Self {
            bpm,
            notes,
            current_time: 0.0,
            score: 0,
            combo: 0,
            max_combo: 0,
            judgments: Vec::new(),
            timers,
        }
    }

    fn judge(&mut self, note_time: f32, hit_time: f32) -> Judgment {
        let diff = (note_time - hit_time).abs();
        let j = if diff < 0.03 {
            Judgment::Perfect
        } else if diff < 0.08 {
            Judgment::Great
        } else if diff < 0.15 {
            Judgment::Good
        } else {
            Judgment::Miss
        };

        match j {
            Judgment::Perfect => { self.score += 300; self.combo += 1; }
            Judgment::Great => { self.score += 200; self.combo += 1; }
            Judgment::Good => { self.score += 100; self.combo += 1; }
            Judgment::Miss => { self.combo = 0; }
        }
        self.max_combo = self.max_combo.max(self.combo);
        self.judgments.push(j);
        j
    }
}

impl AppCallbacks for RhythmGame {
    fn init(&mut self, _ctx: &mut EngineContext) {
        println!("=== Rhythm Game ({} BPM, {} notes) ===", self.bpm, self.notes.len());
    }

    fn update(&mut self, _ctx: &mut EngineContext, dt: f32) {
        self.current_time += dt;
        self.timers.update(dt);

        // Auto-play: hit notes as they come
        for note in &mut self.notes {
            if note.hit { continue; }
            if self.current_time >= note.time - 0.05 && self.current_time <= note.time + 0.15 {
                // Simulate slight timing variation
                let hit_offset = (self.current_time - note.time).abs();
                let simulated_hit = note.time + hit_offset * 0.5;
                note.hit = true;
                let j = self.judge(note.time, simulated_hit);
                let j_str = match j {
                    Judgment::Perfect => "PERFECT",
                    Judgment::Great => "GREAT",
                    Judgment::Good => "GOOD",
                    Judgment::Miss => "MISS",
                };
                if self.combo % 4 == 0 && self.combo > 0 {
                    println!("  t={:.2}s Lane {} — {j_str} (combo: {})", note.time, note.lane, self.combo);
                }
            }
        }
    }
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = RhythmGame::new();
    runner.init(&mut game);
    runner.run_frames(600, 60.0, &mut game);

    let perfect = game.judgments.iter().filter(|&&j| j == Judgment::Perfect).count();
    let great = game.judgments.iter().filter(|&&j| j == Judgment::Great).count();
    let good = game.judgments.iter().filter(|&&j| j == Judgment::Good).count();
    let miss = game.judgments.iter().filter(|&&j| j == Judgment::Miss).count();

    println!("\n--- Results ---");
    println!("Score: {} | Max Combo: {}", game.score, game.max_combo);
    println!("Perfect: {perfect} | Great: {great} | Good: {good} | Miss: {miss}");
}
