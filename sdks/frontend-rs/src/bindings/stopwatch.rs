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
        test_util::try_init_log();
        let _stopwatch = Stopwatch::new("test");
    }
}
