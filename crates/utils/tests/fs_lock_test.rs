use starbase_sandbox::create_empty_sandbox;
use starbase_utils::fs;
use std::thread;
use std::time::Duration;
use std::time::Instant;

mod fs_lock {
    use super::*;

    mod lock_directory {
        use super::*;
        use std::fs as std_fs;

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

        #[test]
        fn ignores_stale_lock_files() {
            let sandbox = create_empty_sandbox();
            let dir = sandbox.path().join("dir");

            fs::create_dir_all(&dir).unwrap();
            std_fs::write(dir.join(fs::LOCK_FILE), "12345").unwrap();

            assert!(!fs::is_dir_locked(&dir));
        }

        // Every unlock removes the `.lock` file, so each handoff between threads
        // recreates it. If the lock didn't guard against locking an orphaned
        // inode, two threads could enter the critical section at once and lose
        // increments. A correct lock always lands on the exact total.
        #[test]
        fn preserves_mutual_exclusion_under_churn() {
            let sandbox = create_empty_sandbox();
            let dir = sandbox.path().join("dir");
            let counter = dir.join("counter");

            fs::create_dir_all(&dir).unwrap();
            std_fs::write(&counter, "0").unwrap();

            let threads: u64 = 8;
            let iterations: u64 = 50;
            let mut handles = vec![];

            for _ in 0..threads {
                let dir = dir.clone();
                let counter = counter.clone();

                handles.push(thread::spawn(move || {
                    for _ in 0..iterations {
                        let lock = fs::lock_directory(&dir).unwrap();

                        let value: u64 = std_fs::read_to_string(&counter)
                            .unwrap()
                            .trim()
                            .parse()
                            .unwrap();
                        std_fs::write(&counter, (value + 1).to_string()).unwrap();

                        drop(lock);
                    }
                }));
            }

            for handle in handles {
                handle.join().unwrap();
            }

            let value: u64 = std_fs::read_to_string(&counter)
                .unwrap()
                .trim()
                .parse()
                .unwrap();

            assert_eq!(value, threads * iterations);
        }
    }

    mod lock_file {
        use super::*;
        use std::fs as std_fs;

        #[test]
        fn all_wait() {
            let sandbox = create_empty_sandbox();
            let file = sandbox.path().join(".lock");
            let mut handles = vec![];
            let start = Instant::now();

            for i in 0..10 {
                let file_clone = file.clone();

                handles.push(thread::spawn(move || {
                    // Stagger
                    thread::sleep(Duration::from_millis(i * 25));

                    let _lock = fs::lock_file(file_clone).unwrap();

                    thread::sleep(Duration::from_millis(250));
                }));
            }

            for handle in handles {
                handle.join().unwrap();
            }

            let elapsed = start.elapsed();

            assert!(elapsed >= Duration::from_millis(2500));
        }

        #[test]
        fn returns_false_for_unlocked_files() {
            let sandbox = create_empty_sandbox();
            let file = sandbox.path().join("file.txt");

            std_fs::write(&file, "contents").unwrap();

            assert!(!fs::is_file_locked(&file));
        }
    }
}
