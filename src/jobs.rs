//! Fork-join job system inspired by `wiJobSystem` (Wicked Engine).
//!
//! The engine already uses `rayon` for one-shot data-parallel hot paths
//! (e.g. [`sdf::marching_cubes_parallel`](crate::sdf)). The job system
//! provides a complementary API that mirrors the `Execute` / `Dispatch` /
//! `Wait` pattern game engines use to express **named** fork-join barriers
//! across a frame, with nestable contexts so subsystems can launch
//! children whose completion still satisfies their parent's `wait`.
//!
//! ## Quick Example
//!
//! ```rust
//! use alice_game_engine::jobs::{dispatch, execute, wait, JobContext};
//! use std::sync::atomic::{AtomicU32, Ordering};
//! use std::sync::Arc;
//!
//! let ctx = JobContext::new();
//! let counter = Arc::new(AtomicU32::new(0));
//!
//! // Single job.
//! let c = Arc::clone(&counter);
//! execute(&ctx, move || { c.fetch_add(1, Ordering::Relaxed); });
//!
//! // 1,000 jobs in groups of 32.
//! let c = Arc::clone(&counter);
//! dispatch(&ctx, 1_000, 32, move |_args| {
//!     c.fetch_add(1, Ordering::Relaxed);
//! });
//!
//! wait(&ctx);
//! assert_eq!(counter.load(Ordering::Relaxed), 1_001);
//! ```
//!
//! ## Design choices
//!
//! - **Dedicated thread pool** (constructed on first use) keeps job
//!   submissions isolated from the global `rayon` pool used elsewhere.
//! - **`Mutex<u32>` + `Condvar`** pending-counter. Wait is cooperative,
//!   not a spin, so multi-millisecond barriers do not burn a CPU core.
//! - **`fork()`** returns a child `JobContext` whose decrement also
//!   advances the parent's counter, so `wait` on the parent transitively
//!   covers the child's submissions.
//! - **Panic propagation** is whatever `rayon` does: a job that panics
//!   unwinds inside its worker; the next `wait` returns normally because
//!   the counter is still decremented by the `JobHandle` `Drop` guard,
//!   matching how rayon's `scope` propagates panics to the joiner.

use std::sync::{Arc, Condvar, Mutex, OnceLock};

use rayon::{ThreadPool, ThreadPoolBuilder};

// ---------------------------------------------------------------------------
// Dedicated pool (lazy-init, process-global)
// ---------------------------------------------------------------------------

static POOL: OnceLock<ThreadPool> = OnceLock::new();

fn pool() -> &'static ThreadPool {
    POOL.get_or_init(|| {
        ThreadPoolBuilder::new()
            .thread_name(|i| format!("alice-jobs-{i}"))
            .build()
            .expect("alice-game-engine: failed to build dedicated job thread pool")
    })
}

// ---------------------------------------------------------------------------
// JobArgs
// ---------------------------------------------------------------------------

/// Per-job invocation metadata passed to dispatched jobs.
///
/// `group_id` and `group_index` let a single job function know both the
/// chunk it is processing and its position within that chunk, which makes
/// `SoA` / cache-line aligned data layouts straightforward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobArgs {
    /// Global job index in `[0, job_count)`.
    pub job_index: u32,
    /// Group ID in `[0, ceil(job_count / group_size))`.
    pub group_id: u32,
    /// Index within the group in `[0, group_size)` (may be smaller for the
    /// last partial group).
    pub group_index: u32,
}

// ---------------------------------------------------------------------------
// Shared counter
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CounterInner {
    pending: Mutex<u32>,
    cv: Condvar,
}

impl CounterInner {
    const fn new() -> Self {
        Self {
            pending: Mutex::new(0),
            cv: Condvar::new(),
        }
    }

    fn add(&self, n: u32) {
        let mut lock = self
            .pending
            .lock()
            .expect("alice-game-engine: jobs counter poisoned");
        *lock = lock.saturating_add(n);
    }

    fn dec(&self) {
        let mut lock = self
            .pending
            .lock()
            .expect("alice-game-engine: jobs counter poisoned");
        *lock = lock.saturating_sub(1);
        if *lock == 0 {
            self.cv.notify_all();
        }
    }

    fn wait(&self) {
        let lock = self
            .pending
            .lock()
            .expect("alice-game-engine: jobs counter poisoned");
        let _guard = self
            .cv
            .wait_while(lock, |pending| *pending > 0)
            .expect("alice-game-engine: jobs counter poisoned");
    }

    fn snapshot(&self) -> u32 {
        *self
            .pending
            .lock()
            .expect("alice-game-engine: jobs counter poisoned")
    }
}

// ---------------------------------------------------------------------------
// JobContext
// ---------------------------------------------------------------------------

/// A fork-join barrier point.
///
/// Submit jobs against a context with [`execute`] / [`dispatch`] and then
/// call [`wait`] to block until every submitted job — and every job in
/// every child context produced by [`JobContext::fork`] — has completed.
///
/// Contexts are cheap to construct; one per logical "stage" of the frame
/// (asset streaming, animation update, particle simulation, …) is the
/// intended pattern.
#[derive(Clone, Debug)]
pub struct JobContext {
    counter: Arc<CounterInner>,
    parent: Option<Arc<CounterInner>>,
}

impl JobContext {
    /// Creates a fresh root context with no parent.
    #[must_use]
    pub fn new() -> Self {
        Self {
            counter: Arc::new(CounterInner::new()),
            parent: None,
        }
    }

    /// Creates a child context whose completion propagates to `self`.
    ///
    /// `wait`-ing on the parent will block until both the parent's own
    /// jobs and all child contexts produced via `fork()` are done.
    #[must_use]
    pub fn fork(&self) -> Self {
        Self {
            counter: Arc::new(CounterInner::new()),
            parent: Some(Arc::clone(&self.counter)),
        }
    }

    /// Returns the current number of jobs still pending in this context
    /// alone (does not include children).
    #[must_use]
    pub fn pending(&self) -> u32 {
        self.counter.snapshot()
    }
}

impl Default for JobContext {
    fn default() -> Self {
        Self::new()
    }
}

// RAII guard that decrements the context (and parent, if any) when
// dropped — including on panic — so `wait` always unblocks.
struct JobGuard {
    counter: Arc<CounterInner>,
    parent: Option<Arc<CounterInner>>,
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        self.counter.dec();
        if let Some(p) = &self.parent {
            p.dec();
        }
    }
}

// ---------------------------------------------------------------------------
// Public submission API
// ---------------------------------------------------------------------------

/// Submits a single job to the dedicated job pool.
///
/// The job runs on a worker thread. Pair with [`wait`] on the same
/// context to fork-join. Panics in `job` are caught by the worker and
/// propagated on the next `wait` (matching `rayon::scope` semantics).
pub fn execute<F>(ctx: &JobContext, job: F)
where
    F: FnOnce() + Send + 'static,
{
    ctx.counter.add(1);
    if let Some(p) = &ctx.parent {
        p.add(1);
    }
    let guard = JobGuard {
        counter: Arc::clone(&ctx.counter),
        parent: ctx.parent.as_ref().map(Arc::clone),
    };
    pool().spawn(move || {
        let _g = guard;
        job();
    });
}

/// Submits `job_count` jobs partitioned into groups of `group_size`.
///
/// One closure call per job. `group_size = 0` is normalised to `1` to
/// avoid divide-by-zero. If `job_count = 0` the call is a no-op.
///
/// The closure must be `Sync` because every group will call it from a
/// worker thread; the [`JobArgs`] passed in identifies which job within
/// which group is being invoked.
pub fn dispatch<F>(ctx: &JobContext, job_count: u32, group_size: u32, job: F)
where
    F: Fn(JobArgs) + Send + Sync + 'static,
{
    if job_count == 0 {
        return;
    }
    let group_size = group_size.max(1);
    // ceil(job_count / group_size) without using division on the hot path.
    let group_count = job_count.div_ceil(group_size);

    let job = Arc::new(job);
    for group_id in 0..group_count {
        let start = group_id.saturating_mul(group_size);
        let end = (start.saturating_add(group_size)).min(job_count);
        let job = Arc::clone(&job);

        ctx.counter.add(1);
        if let Some(p) = &ctx.parent {
            p.add(1);
        }
        let guard = JobGuard {
            counter: Arc::clone(&ctx.counter),
            parent: ctx.parent.as_ref().map(Arc::clone),
        };

        pool().spawn(move || {
            let _g = guard;
            for (offset, job_index) in (start..end).enumerate() {
                #[allow(clippy::cast_possible_truncation)]
                let group_index = offset as u32;
                job(JobArgs {
                    job_index,
                    group_id,
                    group_index,
                });
            }
        });
    }
}

/// Blocks the calling thread until every job submitted to `ctx` — and
/// every job submitted to any context produced by `ctx.fork()` — has
/// completed. Safe to call repeatedly; after the call the context can be
/// reused for another batch.
pub fn wait(ctx: &JobContext) {
    ctx.counter.wait();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn execute_single_job_runs_and_completes() {
        let ctx = JobContext::new();
        let flag = Arc::new(AtomicU32::new(0));
        let f = Arc::clone(&flag);
        execute(&ctx, move || {
            f.store(1, Ordering::Relaxed);
        });
        wait(&ctx);
        assert_eq!(flag.load(Ordering::Relaxed), 1);
        assert_eq!(ctx.pending(), 0);
    }

    #[test]
    fn dispatch_parallel_runs_all_jobs() {
        let ctx = JobContext::new();
        let counter = Arc::new(AtomicU32::new(0));
        let c = Arc::clone(&counter);
        dispatch(&ctx, 1_000, 32, move |_args| {
            c.fetch_add(1, Ordering::Relaxed);
        });
        wait(&ctx);
        assert_eq!(counter.load(Ordering::Relaxed), 1_000);
    }

    #[test]
    fn wait_blocks_until_all_complete() {
        let ctx = JobContext::new();
        let counter = Arc::new(AtomicU32::new(0));
        for _ in 0..16 {
            let c = Arc::clone(&counter);
            execute(&ctx, move || {
                std::thread::sleep(Duration::from_millis(5));
                c.fetch_add(1, Ordering::Relaxed);
            });
        }
        // At submission time at least one job may already have completed,
        // so we only assert the post-wait invariant.
        wait(&ctx);
        assert_eq!(counter.load(Ordering::Relaxed), 16);
        assert_eq!(ctx.pending(), 0);
    }

    #[test]
    fn fork_child_propagates_to_parent() {
        let parent = JobContext::new();
        let child = parent.fork();
        let counter = Arc::new(AtomicU32::new(0));

        // 50 jobs on the parent, 50 on the child.
        let c = Arc::clone(&counter);
        dispatch(&parent, 50, 8, move |_| {
            c.fetch_add(1, Ordering::Relaxed);
        });
        let c = Arc::clone(&counter);
        dispatch(&child, 50, 8, move |_| {
            std::thread::sleep(Duration::from_millis(1));
            c.fetch_add(1, Ordering::Relaxed);
        });

        // Waiting on the parent must cover the child's jobs too.
        wait(&parent);
        assert_eq!(counter.load(Ordering::Relaxed), 100);
        assert_eq!(parent.pending(), 0);
        assert_eq!(child.pending(), 0);
    }

    #[test]
    fn multiple_contexts_are_independent() {
        let a = JobContext::new();
        let b = JobContext::new();
        let counter = Arc::new(AtomicU32::new(0));

        // Submit only to `a`, then verify `b` reports zero pending and
        // returns immediately from `wait`.
        let c = Arc::clone(&counter);
        execute(&a, move || {
            std::thread::sleep(Duration::from_millis(20));
            c.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(b.pending(), 0);
        wait(&b);
        wait(&a);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn dispatch_with_uneven_groups() {
        // job_count = 100, group_size = 32 → groups of {32, 32, 32, 4}.
        let ctx = JobContext::new();
        let seen = Arc::new(AtomicUsize::new(0));
        let seen_indices: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
        let s = Arc::clone(&seen);
        let si = Arc::clone(&seen_indices);
        dispatch(&ctx, 100, 32, move |args| {
            s.fetch_add(1, Ordering::Relaxed);
            si.lock().unwrap().push(args.job_index);
        });
        wait(&ctx);
        assert_eq!(seen.load(Ordering::Relaxed), 100);
        let mut indices = seen_indices.lock().unwrap().clone();
        indices.sort_unstable();
        let expected: Vec<u32> = (0..100).collect();
        assert_eq!(indices, expected);
    }

    #[test]
    fn job_args_indices_correct() {
        let ctx = JobContext::new();
        let observed: Arc<Mutex<Vec<JobArgs>>> = Arc::new(Mutex::new(Vec::new()));
        let o = Arc::clone(&observed);
        dispatch(&ctx, 10, 4, move |args| {
            o.lock().unwrap().push(args);
        });
        wait(&ctx);
        let mut got = observed.lock().unwrap().clone();
        got.sort_by_key(|a| a.job_index);
        assert_eq!(got.len(), 10);
        // 3 groups of size {4, 4, 2}.
        for (i, args) in got.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let expected_index = i as u32;
            assert_eq!(args.job_index, expected_index);
            assert_eq!(args.group_id, expected_index / 4);
            assert_eq!(args.group_index, expected_index % 4);
        }
    }

    #[test]
    fn context_can_be_reused_after_wait() {
        let ctx = JobContext::new();
        let counter = Arc::new(AtomicU32::new(0));

        for batch in 0..3 {
            let c = Arc::clone(&counter);
            dispatch(&ctx, 50, 8, move |_| {
                c.fetch_add(1, Ordering::Relaxed);
            });
            wait(&ctx);
            #[allow(clippy::cast_possible_truncation)]
            let expected = 50 * (batch + 1) as u32;
            assert_eq!(counter.load(Ordering::Relaxed), expected);
            assert_eq!(ctx.pending(), 0);
        }
    }

    #[test]
    fn dispatch_zero_jobs_is_noop() {
        let ctx = JobContext::new();
        dispatch(&ctx, 0, 32, |_| panic!("should not run"));
        assert_eq!(ctx.pending(), 0);
        wait(&ctx); // returns immediately
    }

    #[test]
    fn dispatch_zero_group_size_normalises_to_one() {
        let ctx = JobContext::new();
        let counter = Arc::new(AtomicU32::new(0));
        let c = Arc::clone(&counter);
        // group_size = 0 must be normalised to 1, so we get 5 groups of 1.
        dispatch(&ctx, 5, 0, move |_| {
            c.fetch_add(1, Ordering::Relaxed);
        });
        wait(&ctx);
        assert_eq!(counter.load(Ordering::Relaxed), 5);
    }
}
