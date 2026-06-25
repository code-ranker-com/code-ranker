use code_ranker_plugin_api::log;
use std::time::Instant;

/// Always shown, even at `--output.mode quiet` (errors).
pub fn error(msg: &str) {
    log::line(msg);
}

/// Shown at `summary`+ : warnings and written-artifact confirmations.
pub fn summary(msg: &str) {
    log::summary(msg);
}

/// Shown only at `--output.mode verbose` : the `▶` command echo and `config:` line.
pub fn verbose(msg: &str) {
    log::verbose(msg);
}

pub struct Timer {
    label: String,
    start: Instant,
}

impl Timer {
    pub fn start(label: &str) -> Self {
        Self {
            label: label.to_string(),
            start: Instant::now(),
        }
    }

    pub fn finish_with(self, extra: &str) -> u64 {
        let elapsed = self.start.elapsed();
        log::summary(&format!(
            "✓ {} — {}{}",
            self.label,
            log::secs(elapsed),
            if extra.is_empty() {
                String::new()
            } else {
                format!(" ({})", extra)
            }
        ));
        elapsed.as_millis() as u64
    }

    pub fn finish(self) -> u64 {
        self.finish_with("")
    }

    /// Measure the elapsed time without printing — for per-stage timers whose
    /// numbers are recorded in the snapshot but kept out of the console output.
    pub fn finish_quiet(self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}
