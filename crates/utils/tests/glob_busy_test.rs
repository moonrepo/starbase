#![cfg(feature = "glob")]

use starbase_sandbox::create_empty_sandbox;
use starbase_utils::glob::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex, MutexGuard};
use std::time::Duration;

// Only one test may block the pool at a time, otherwise the workers of
// the second test would never start, and it would wait for them forever
static POOL: Mutex<()> = Mutex::new(());

/// Blocks every thread in rayon's global pool, so that jwalk is unable to
/// spawn its walking task and aborts to avoid a possible deadlock. This is
/// what happens when many parallel walks share the same pool.
struct BusyPool {
    done: Arc<AtomicBool>,
    exited: Arc<Barrier>,
    _guard: MutexGuard<'static, ()>,
}

impl BusyPool {
    fn saturate() -> Self {
        let guard = POOL.lock().unwrap_or_else(|error| error.into_inner());
        let threads = rayon::current_num_threads();
        let blocked = Arc::new(Barrier::new(threads + 1));
        let exited = Arc::new(Barrier::new(threads + 1));
        let done = Arc::new(AtomicBool::new(false));

        for _ in 0..threads {
            let blocked = Arc::clone(&blocked);
            let exited = Arc::clone(&exited);
            let done = Arc::clone(&done);

            rayon::spawn(move || {
                blocked.wait();

                while !done.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(5));
                }

                exited.wait();
            });
        }

        // Wait until every worker is actually blocked
        blocked.wait();

        Self {
            done,
            exited,
            _guard: guard,
        }
    }
}

impl Drop for BusyPool {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);

        // Don't release the pool until the workers have stopped
        self.exited.wait();
    }
}

#[test]
fn retries_serially_instead_of_returning_no_files() {
    let sandbox = create_empty_sandbox();
    sandbox.create_file("a/1.txt", "");
    sandbox.create_file("a/b/2.txt", "");
    sandbox.create_file("3.txt", "");

    let _pool = BusyPool::saturate();
    let mut paths = walk_fast(sandbox.path(), ["**/*.txt"]).unwrap();
    paths.sort();

    assert_eq!(
        paths,
        vec![
            sandbox.path().join("3.txt"),
            sandbox.path().join("a/1.txt"),
            sandbox.path().join("a/b/2.txt"),
        ]
    );
}

#[test]
fn retries_with_options_and_negations() {
    let sandbox = create_empty_sandbox();
    sandbox.create_file("src/1.txt", "");
    sandbox.create_file("dist/2.txt", "");

    let _pool = BusyPool::saturate();
    let paths = walk_fast_with_options(
        sandbox.path(),
        ["**/*.txt", "!dist/**"],
        GlobWalkOptions::default().files(),
    )
    .unwrap();

    assert_eq!(paths, vec![sandbox.path().join("src/1.txt")]);
}
