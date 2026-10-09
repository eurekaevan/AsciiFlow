use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    commit_gate: Arc<Mutex<()>>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        let _gate = self
            .commit_gate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.cancelled.store(true, Ordering::Release);
    }

    /// Linearize cancellation against a final, short atomic commit operation.
    /// A request admitted first prevents commit; an admitted commit completes
    /// before a later request. Do not run pipeline work or callbacks here.
    pub fn commit_unless_cancelled<T>(&self, commit: impl FnOnce() -> T) -> Option<T> {
        let _gate = self
            .commit_gate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.is_cancelled() {
            None
        } else {
            Some(commit())
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_admitted_first_prevents_commit() {
        let token = CancellationToken::new();
        token.cancel();
        assert_eq!(
            token.commit_unless_cancelled(|| panic!("must not commit")),
            None::<()>
        );
    }
    #[test]
    fn admitted_commit_finishes_before_cancellation() {
        let token = CancellationToken::new();
        let other = token.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            started_rx.recv().unwrap();
            other.cancel();
        });
        assert_eq!(
            token.commit_unless_cancelled(|| {
                started_tx.send(()).unwrap();
                assert!(!token.is_cancelled());
                "committed"
            }),
            Some("committed")
        );
        handle.join().unwrap();
        assert!(token.is_cancelled());
    }
}
