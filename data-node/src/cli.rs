use clap::Parser;
use config::Configuration;
use std::{
    error::Error,
    fs::File,
    io::{self, ErrorKind},
    path::Path,
};

#[derive(Debug, Parser, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to the configuration file in YAML format.
    #[arg(short, long)]
    config: String,
}

impl Cli {
    pub fn create_config(&self) -> Result<Configuration, Box<dyn Error>> {
        let path = Path::new(&self.config);
        if !path.exists() {
            eprintln!(
                "The specified configuration file `{}` does not exist",
                self.config
            );

            return Err(Box::new(io::Error::new(
                ErrorKind::NotFound,
                self.config.clone(),
            )));
        }

        let config_file = File::open(path)?;
        let mut configuration: Configuration = serde_yaml::from_reader(config_file)?;
        configuration.check_and_apply()?;

        Ok(configuration)
    }
}
