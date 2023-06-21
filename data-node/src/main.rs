use clap::Parser;
use data_node::Cli;
use log::info;
use tokio::sync::broadcast;

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() {
    let cli = Cli::parse();
    cli.init_log().unwrap();

    display_built_info();

    let config = match cli.create_config() {
        Ok(config) => config,
        Err(e) => {
            eprintln!(
                "Failed to create configuration from the specified configuration file. Cause: {:?}",
                e
            );
            return;
        }
    };

    let (shutdown_tx, _rx) = broadcast::channel(1);
    let tx = shutdown_tx.clone();
    ctrlc::set_handler(move || {
        println!("Received shutdown signal");
        if tx.send(()).is_err() {
            eprintln!("Could not send shutdown signal to shutdown channel");
        }
    })
    .expect("Failed to set Ctrl-C");

    if let Err(e) = data_node::server::launch(config, shutdown_tx) {
        eprintln!("Failed to start data-node: {:?}", e);
    }
}

// Additively prints the built info to both stdout and log.
macro_rules! build_info {
    ($($st:tt)*) => {
        println!($($st)*);
        info!($($st)*);
    };
}

/// Display the built information.
///
/// The build information is additively printed to both stdout and log.
fn display_built_info() {
    build_info!(
        "Data-Node v{}, built for {} by {}.",
        data_node::built_info::PKG_VERSION,
        data_node::built_info::TARGET,
        data_node::built_info::RUSTC_VERSION
    );

    let built_time = built::util::strptime(data_node::built_info::BUILT_TIME_UTC);
    build_info!(
        "Built with profile \"{}\", on {} ({} days ago).",
        data_node::built_info::PROFILE,
        built_time.with_timezone(&built::chrono::offset::Local),
        (built::chrono::offset::Utc::now() - built_time).num_days(),
    );

    if let (Some(v), Some(dirty), Some(hash), Some(short_hash)) = (
        data_node::built_info::GIT_VERSION,
        data_node::built_info::GIT_DIRTY,
        data_node::built_info::GIT_COMMIT_HASH,
        data_node::built_info::GIT_COMMIT_HASH_SHORT,
    ) {
        build_info!(
            "Built from git `{}`, commit {}, short_commit {}; the working directory was \"{}\".",
            v,
            hash,
            short_hash,
            if dirty { "dirty" } else { "clean" }
        );
    }

    if let Some(r) = data_node::built_info::GIT_HEAD_REF {
        build_info!("The branch was `{r}`.");
    }
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        time::{Duration, Instant},
    };

    use tokio::sync::broadcast;

    #[test]
    fn test_broadcast() -> Result<(), Box<dyn Error>> {
        let (tx, _rx) = broadcast::channel(1);

        let handles = (0..3)
            .map(|i| {
                let mut shutdown_rx = tx.subscribe();
                std::thread::Builder::new()
                    .name(format!("thread-{}", i))
                    .spawn(move || loop {
                        let start = Instant::now();
                        if start.elapsed() > Duration::from_secs(1) {
                            panic!("Should have received signal");
                        }
                        match shutdown_rx.try_recv() {
                            Ok(_) => break,
                            Err(_e) => {
                                std::thread::sleep(Duration::from_millis(10));
                            }
                        }
                    })
            })
            .collect::<Vec<_>>();

        tx.send(())?;

        for handle in handles {
            handle?.join().unwrap();
        }

        Ok(())
    }
}
