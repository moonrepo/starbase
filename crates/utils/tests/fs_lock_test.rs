use starbase_sandbox::create_empty_sandbox;
use starbase_utils::fs;
use std::thread;
use std::time::Duration;
use std::time::Instant;

mod fs_lock {
    use super::*;

    mod lock_directory {
        use super::*;

        #[test]
        fn all_wait() {
            let sandbox = create_empty_sandbox();
            let dir = sandbox.path().join("dir");
            let mut handles = vec![];
            let start = Instant::now();

            for i in 0..10 {
                let dir_clone = dir.clone();

                handles.push(thread::spawn(move || {
                    // Stagger
                    thread::sleep(Duration::from_millis(i * 25));

                    let _lock = fs::lock_directory(dir_clone).unwrap();

                    thread::sleep(Duration::from_millis(250));
                }));
            }

            for handle in handles {
                handle.join().unwrap();
            }

            let elapsed = start.elapsed();

            assert!(elapsed >= Duration::from_millis(2500));
        }
    }
}
