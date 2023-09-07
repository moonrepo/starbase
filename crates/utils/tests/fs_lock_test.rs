use starbase_sandbox::create_empty_sandbox;
use starbase_utils::fs;
use std::thread;
use std::time::Duration;

mod fs_lock {
    use super::*;

    #[tokio::test]
    async fn async_lock_directory_all_wait() {
        let sandbox = create_empty_sandbox();
        let dir = sandbox.path().join("dir");
        let mut futures = vec![];

        for _ in 0..10 {
            let dir_clone = dir.clone();

            futures.push(tokio::spawn(async move {
                let lock = fs::lock_directory(dir_clone).await.unwrap();

                tokio::time::sleep(Duration::from_millis(500)).await;

                lock.unlock().unwrap();
            }));
        }

        for future in futures {
            future.await.unwrap();
        }
    }

    #[test]
    fn sync_lock_directory_all_wait() {
        let sandbox = create_empty_sandbox();
        let dir = sandbox.path().join("dir");
        let mut handles = vec![];

        for _ in 0..10 {
            let dir_clone = dir.clone();

            handles.push(thread::spawn(|| {
                let lock = fs::lock_directory_blocking(dir_clone).unwrap();

                thread::sleep(Duration::from_millis(500));

                lock.unlock().unwrap();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }
}
