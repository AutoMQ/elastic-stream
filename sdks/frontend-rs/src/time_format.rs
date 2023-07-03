#[cfg(test)]
mod tests {
    use std::error::Error;

    use log::debug;

    #[test]
    fn test_time_format() -> Result<(), Box<dyn Error>> {
        crate::log::try_init_log();
        debug!("DateTime format");
        Ok(())
    }
}
