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

    mod with_file {
        use super::*;
        use std::fs as std_fs;

        // Same churn shape as `lock_directory`, but entering through
        // `FileLock::with_file` with caller-opened handles, to exercise its
        // stale-handle fallback path.
        #[test]
        fn preserves_mutual_exclusion_under_churn() {
            let sandbox = create_empty_sandbox();
            let lock_path = sandbox.path().join(".lock");
            let counter = sandbox.path().join("counter");

            std_fs::write(&counter, "0").unwrap();

            let threads: u64 = 8;
            let iterations: u64 = 50;
            let mut handles = vec![];

            for _ in 0..threads {
                let lock_path = lock_path.clone();
                let counter = counter.clone();

                handles.push(thread::spawn(move || {
                    for _ in 0..iterations {
                        let file = std_fs::OpenOptions::new()
                            .read(true)
                            .write(true)
                            .create(true)
                            .truncate(false)
                            .open(&lock_path)
                            .unwrap();

                        let mut lock = fs::FileLock::with_file(lock_path.clone(), file).unwrap();
                        lock.remove_on_unlock();

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

    mod new_async {
        use super::*;

        #[test]
        fn locks_and_unlocks() {
            let sandbox = create_empty_sandbox();
            let dir = sandbox.path().join("dir");

            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();

            runtime.block_on(async {
                let path = dir.join(fs::LOCK_FILE);

                fs::create_dir_all(&dir).unwrap();

                let mut lock = fs::FileLock::new_async(path).await.unwrap();
                lock.remove_on_unlock();

                assert!(fs::is_dir_locked(&dir));

                drop(lock);

                assert!(!fs::is_dir_locked(&dir));
            });
        }
    }

    mod read_write_file_with_lock {
        use super::*;

        // Writers replace the entire content with a single repeated byte;
        // readers must never observe a torn (mixed or partial) payload.
        #[test]
        fn readers_never_observe_torn_writes() {
            let sandbox = create_empty_sandbox();
            let file = sandbox.path().join("data");
            let size = 4096;

            fs::write_file_with_lock(&file, "a".repeat(size)).unwrap();

            let mut handles = vec![];

            for value in ["b", "c", "d"] {
                let file = file.clone();

                handles.push(thread::spawn(move || {
                    for _ in 0..25 {
                        fs::write_file_with_lock(&file, value.repeat(size)).unwrap();
                    }
                }));
            }

            for _ in 0..3 {
                let file = file.clone();

                handles.push(thread::spawn(move || {
                    for _ in 0..25 {
                        let content = fs::read_file_with_lock(&file).unwrap();
                        let mut chars = content.chars();
                        let first = chars.next().unwrap();

                        assert_eq!(content.len(), size);
                        assert!(chars.all(|ch| ch == first));
                    }
                }));
            }

            for handle in handles {
                handle.join().unwrap();
            }
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
