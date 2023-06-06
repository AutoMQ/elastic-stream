use clap::Parser;
use config::Configuration;
use log::info;
use std::{fs::File, path::Path};

#[derive(Debug, Parser, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to the configuration file in YAML format.
    #[arg(short, long, env="DATA_NODE_CONFIG")]
    config: String,

    /// Path to the log4rs configuration file in YAML format.
    #[arg(short, long, env="DATA_NODE_LOG_CONFIG")]
    log: String,
}

impl Cli {
    pub fn init_log(&self) -> anyhow::Result<()> {
        let log_config = Path::new(&self.log);
        if !log_config.exists() {
            eprintln!("The specified log configuration file does not exist");
            // Exit with errno set
            std::process::exit(2);
        }

        if !log_config.is_file() {
            eprintln!("The specified log configuration path is not a file");
            // Exit with errno set
            std::process::exit(22);
        }

        log4rs::init_file(log_config, Default::default())?;
        info!("Log initialized");
        Ok(())
    }

    pub fn create_config(&self) -> anyhow::Result<Configuration> {
        let path = Path::new(&self.config);
        if !path.exists() {
            eprintln!(
                "The specified configuration file `{}` does not exist",
                self.config
            );
            // Exit with errno set
            std::process::exit(2);
        }

        let config_file = File::open(path)?;
        let mut configuration: Configuration = serde_yaml::from_reader(config_file)?;
        configuration.check_and_apply()?;

        Ok(configuration)
    }
}
