use log::trace;
use minstant::Instant;

pub(crate) struct Stopwatch<'a> {
    name: &'a str,
    start: Instant,
}

impl<'a> Stopwatch<'a> {
    pub(crate) fn new(name: &'a str) -> Stopwatch<'a> {
        trace!("{} starts", name);
        Stopwatch {
            name,
            start: Instant::now(),
        }
    }
}

impl<'a> Drop for Stopwatch<'a> {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        trace!("{} took {}us", self.name, elapsed.as_micros());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stopwatch() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _stopwatch = Stopwatch::new("test");
    }
}
