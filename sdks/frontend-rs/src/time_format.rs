#[cfg(test)]
mod tests {
    use std::error::Error;

    use log::debug;

    #[test]
    fn test_time_format() -> Result<(), Box<dyn Error>> {
        test_util::try_init_log();
        debug!("DateTime format");
        Ok(())
    }
}
