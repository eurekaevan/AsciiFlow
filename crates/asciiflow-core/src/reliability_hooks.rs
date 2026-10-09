//! Explicit qualification-only synchronization; absent from normal builds.
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

type Observer = Arc<dyn Fn(&str) + Send + Sync>;
static OBSERVER: Mutex<Option<Observer>> = Mutex::new(None);
static FAILURE: Mutex<Option<&'static str>> = Mutex::new(None);

/// A qualification-only error at a named production checkpoint.
pub struct InjectedFailure;
impl InjectedFailure {
    pub fn at(phase: &'static str) -> Self {
        let mut failure = FAILURE.lock().unwrap();
        assert!(failure.is_none());
        *failure = Some(phase);
        Self
    }
}
impl Drop for InjectedFailure {
    fn drop(&mut self) {
        *FAILURE.lock().unwrap() = None;
    }
}
pub fn failure(phase: &str) -> crate::Result<()> {
    if FAILURE
        .lock()
        .unwrap()
        .is_some_and(|target| target == phase)
        || std::env::var("ASCIIFLOW_RELIABILITY_FAILURE").is_ok_and(|target| target == phase)
    {
        Err(crate::Error::Media(format!(
            "qualification injected {phase} error"
        )))
    } else {
        Ok(())
    }
}

/// Install one process-scoped test observer. Run qualification jobs serially.
pub struct Observation;
impl Observation {
    pub fn install(observer: impl Fn(&str) + Send + Sync + 'static) -> Self {
        let mut slot = OBSERVER.lock().unwrap();
        assert!(slot.is_none(), "qualification observer already installed");
        *slot = Some(Arc::new(observer));
        Self
    }
}
impl Drop for Observation {
    fn drop(&mut self) {
        *OBSERVER.lock().unwrap() = None;
    }
}

/// Invoke outside native locks; IPC gates have a bounded failure watchdog.
pub fn checkpoint(phase: &str) {
    let observer = OBSERVER.lock().unwrap().clone();
    if let Some(observer) = observer {
        observer(phase);
    }
    #[cfg(unix)]
    if std::env::var("ASCIIFLOW_RELIABILITY_PHASE").is_ok_and(|target| target == phase) {
        static HIT: AtomicBool = AtomicBool::new(false);
        if !HIT.swap(true, Ordering::AcqRel) {
            use std::io::Read;
            {
                let mut stream =
                    connect(phase).expect("qualification phase requires controller socket");
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(30)))
                    .unwrap();
                let mut ack = [0];
                stream
                    .read_exact(&mut ack)
                    .expect("qualification gate watchdog expired");
                assert_eq!(ack, [1], "qualification gate rejected");
            }
        }
    }
}

/// Notification only: never wait inside a cancellation/commit lock.
pub fn cancellation_observed() {
    #[cfg(unix)]
    let _ = connect("CancellationRequested");
}

#[cfg(unix)]
fn connect(phase: &str) -> Option<std::os::unix::net::UnixStream> {
    use std::io::Write;
    let path = std::env::var_os("ASCIIFLOW_RELIABILITY_SOCKET")?;
    let mut stream = std::os::unix::net::UnixStream::connect(path)
        .expect("qualification controller unavailable");
    stream.write_all(format!("{phase}\n").as_bytes()).unwrap();
    Some(stream)
}
