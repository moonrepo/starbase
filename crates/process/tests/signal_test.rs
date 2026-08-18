use starbase_process::SignalType;

mod signal_type {
    use super::*;

    #[test]
    fn maps_to_unix_codes() {
        assert_eq!(SignalType::Interrupt.get_code(), 2);
        assert_eq!(SignalType::Quit.get_code(), 3);
        assert_eq!(SignalType::Kill.get_code(), 9);
        assert_eq!(SignalType::Terminate.get_code(), 15);
    }
}

#[cfg(unix)]
mod kill {
    use super::*;
    use starbase_process::kill;

    #[tokio::test]
    async fn signals_a_running_process() {
        let mut child = tokio::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id().unwrap();

        kill(pid, SignalType::Kill).unwrap();

        assert!(!child.wait().await.unwrap().success());
    }

    #[tokio::test]
    async fn missing_processes_are_not_an_error() {
        // The process may have exited on its own before we signal it,
        // which the kill helper treats as a success (ESRCH)
        let mut child = tokio::process::Command::new("true").spawn().unwrap();
        let pid = child.id().unwrap();

        child.wait().await.unwrap();

        assert!(kill(pid, SignalType::Terminate).is_ok());
    }
}
