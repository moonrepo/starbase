#![cfg(feature = "glob")]

use starbase_sandbox::create_empty_sandbox;
use starbase_utils::glob::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};

/// Blocks every thread in rayon's global pool, so that jwalk is unable to
/// spawn its walking task and aborts to avoid a possible deadlock. This is
/// what happens when many parallel walks share the same pool.
struct BusyPool {
    done: Arc<AtomicBool>,
}

impl BusyPool {
    fn saturate() -> Self {
        let threads = rayon::current_num_threads();
        let barrier = Arc::new(Barrier::new(threads + 1));
        let done = Arc::new(AtomicBool::new(false));

        for _ in 0..threads {
            let barrier = Arc::clone(&barrier);
            let done = Arc::clone(&done);

            rayon::spawn(move || {
                barrier.wait();

                while !done.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            });
        }

        // Wait until every worker is actually blocked
        barrier.wait();

        Self { done }
    }
}

impl Drop for BusyPool {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
    }
}

#[test]
fn errors_instead_of_returning_no_files() {
    let sandbox = create_empty_sandbox();
    sandbox.create_file("a/1.txt", "");
    sandbox.create_file("a/b/2.txt", "");
    sandbox.create_file("3.txt", "");

    let _pool = BusyPool::saturate();
    let error = walk_fast(sandbox.path(), ["**/*.txt"]).unwrap_err();

    assert!(
        matches!(error, GlobError::WalkAborted { .. }),
        "expected an aborted walk, got {error:?}"
    );
}
