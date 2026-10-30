use std::io::{IsTerminal as _, Write as _};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use kestrel_core::report::report_error;
use kestrel_core::transfer::Progress;

pub(super) struct Monitor {
    pub progress: Progress,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Monitor {
    pub fn new() -> Self {
        let progress = Progress::default();
        let stopped = Arc::new(AtomicBool::new(false));
        let worker = if std::io::stderr().is_terminal() {
            let progress = progress.clone();
            let stop = stopped.clone();
            Some(thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let state = progress.snapshot();
                    let filled = state.percent().map(|percent| (percent * 0.24) as usize).unwrap_or(0);
                    let bar = format!("{}{}", "=".repeat(filled), " ".repeat(24 - filled));
                    eprint!(
                        "\r\x1b[2K[{}] {}: {} / {}",
                        bar,
                        state.stage,
                        state.completed,
                        state.total.map(|total| total.to_string()).unwrap_or_else(|| "?".into())
                    );
                    if let Err(error) = std::io::stderr().flush() {
                        report_error("show transfer progress", error);
                        break;
                    }
                    thread::sleep(Duration::from_millis(80));
                }
                eprint!("\r\x1b[2K");
            }))
        } else {
            None
        };
        Self { progress, stopped, worker }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            report_error("stop transfer progress", "the progress renderer panicked");
        }
    }
}
