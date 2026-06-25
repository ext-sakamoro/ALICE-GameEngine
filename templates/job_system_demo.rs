//! Job system demo — shows fork-join nesting across three stages of a
//! mock game frame:
//!
//! 1. `parent` context with two `execute` jobs (asset preload simulation)
//! 2. `child` forked off `parent` that dispatches 256 jobs (particle
//!    simulation simulation)
//! 3. Single `wait(&parent)` blocks until everything finishes; the child
//!    context's progress propagates to its parent.
//!
//! ```bash
//! cargo run --example job_system_demo
//! ```

use alice_game_engine::jobs::{dispatch, execute, wait, JobContext};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    println!("=== Job System Demo ===");

    let parent = JobContext::new();
    let child = parent.fork();

    let asset_jobs_done = Arc::new(AtomicU32::new(0));
    let particle_jobs_done = Arc::new(AtomicU32::new(0));
    let max_observed_group_index = Arc::new(AtomicU32::new(0));

    let start = Instant::now();

    // Stage 1: two sequential-looking "asset preload" jobs on the parent.
    for asset_id in 0..2u32 {
        let done = Arc::clone(&asset_jobs_done);
        execute(&parent, move || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            done.fetch_add(1, Ordering::Relaxed);
            println!("  asset {asset_id} preloaded");
        });
    }

    // Stage 2: 256 "particle" jobs in groups of 32 on the child context.
    let done = Arc::clone(&particle_jobs_done);
    let maxg = Arc::clone(&max_observed_group_index);
    dispatch(&child, 256, 32, move |args| {
        done.fetch_add(1, Ordering::Relaxed);
        // Track the largest group_index we ever saw so the demo can print
        // the actual fan-out the system performed.
        let prev = maxg.load(Ordering::Relaxed);
        if args.group_index > prev {
            maxg.fetch_max(args.group_index, Ordering::Relaxed);
        }
    });

    // Single wait on the parent covers both stages.
    wait(&parent);
    let elapsed = start.elapsed();

    println!(
        "asset jobs done: {} / 2",
        asset_jobs_done.load(Ordering::Relaxed),
    );
    println!(
        "particle jobs done: {} / 256",
        particle_jobs_done.load(Ordering::Relaxed),
    );
    println!(
        "max observed group_index: {} (group_size = 32, so expected 31)",
        max_observed_group_index.load(Ordering::Relaxed),
    );
    println!("parent.pending(): {}", parent.pending());
    println!("child.pending():  {}", child.pending());
    println!("elapsed: {elapsed:?}");
}
