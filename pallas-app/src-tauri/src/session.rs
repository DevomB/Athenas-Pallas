use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

pub struct AppSession {
    pub running: AtomicBool,
    pub cancel: Arc<AtomicBool>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl AppSession {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            cancel: Arc::new(AtomicBool::new(false)),
            join: Mutex::new(None),
        }
    }

    pub fn try_start(&self) -> bool {
        !self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
    }

    pub fn finish_run(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.cancel.store(false, Ordering::SeqCst);
    }

    pub fn stop_run(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn set_join(&self, handle: JoinHandle<()>) {
        *self.join.lock().unwrap() = Some(handle);
    }

    pub fn shutdown(&self) {
        self.stop_run();
        let handle = self.join.lock().unwrap().take();
        if let Some(h) = handle {
            let _ = h.join();
        }
        self.running.store(false, Ordering::SeqCst);
        self.cancel.store(false, Ordering::SeqCst);
    }

    pub fn join_with_timeout(&self, timeout: Duration) -> bool {
        let handle = self.join.lock().unwrap().take();
        if let Some(h) = handle {
            let done = Arc::new(AtomicBool::new(false));
            let done_worker = done.clone();
            let waiter = std::thread::spawn(move || {
                let _ = h.join();
                done_worker.store(true, Ordering::SeqCst);
            });
            let start = std::time::Instant::now();
            while !done.load(Ordering::SeqCst) && start.elapsed() < timeout {
                std::thread::sleep(Duration::from_millis(50));
            }
            if !done.load(Ordering::SeqCst) {
                drop(waiter);
                self.cancel.store(true, Ordering::SeqCst);
                return false;
            }
        }
        self.running.store(false, Ordering::SeqCst);
        true
    }
}

impl Default for AppSession {
    fn default() -> Self {
        Self::new()
    }
}
