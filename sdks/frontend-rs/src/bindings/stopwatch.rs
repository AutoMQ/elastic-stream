use std::time::Duration;

use log::{trace,warn};
use minstant::Instant;

pub(crate) struct Stopwatch<'a> {
    name: &'a str,
    start: Instant,
    warn_duration: Duration,
}

impl<'a> Stopwatch<'a> {
    pub(crate) fn new(name: &'a str, warn_duration: Duration) -> Stopwatch<'a> {
        trace!("{} starts", name);
        Stopwatch {
            name,
            start: Instant::now(),
            warn_duration,
        }
    }
}

impl<'a> Drop for Stopwatch<'a> {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        if elapsed > self.warn_duration {
            warn!("{} took {}us", self.name, elapsed.as_micros());
        } else {
            trace!("{} took {}us", self.name, elapsed.as_micros());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stopwatch() {
        test_util::try_init_log();
        let _stopwatch = Stopwatch::new("test", Duration::from_millis(1));
    }
}
