use std::{cmp::Ordering, os::unix::prelude::OsStrExt, sync::Arc};

use config::Configuration;
use slog::{error, info, Logger};
use tokio::sync::broadcast::{self, error::TryRecvError};

pub(crate) fn generate_flame_graph(
    config: Arc<Configuration>,
    log: Logger,
    mut shutdown: broadcast::Receiver<()>,
) {
    std::thread::Builder::new()
        .name("flamegraph".to_owned())
        .spawn(move || {
            // Bind to CPU processor 0.
            core_affinity::set_for_current(core_affinity::CoreId { id: 0 });

            let guard = match pprof::ProfilerGuardBuilder::default()
                .frequency(config.server.profiling.sampling_frequency)
                .blocklist(&["libc", "libgcc", "pthread", "vdso"])
                .build()
            {
                Ok(guard) => guard,
                Err(e) => {
                    error!(log, "Failed to build profiler guard: {}", e);
                    return;
                }
            };

            let mut last_generation_instant = std::time::Instant::now();
            loop {
                match shutdown.try_recv() {
                    Ok(_) => {
                        info!(log, "Received TERM/STOP signal, quit profiler");
                        break;
                    }
                    Err(TryRecvError::Closed) => {
                        info!(log, "Received TERM/STOP signal, quit profiler");
                        break;
                    }
                    Err(e) => {
                        error!(
                            log,
                            "Unexpected error while try_recv from shutdown channel: {}", e
                        );
                    }
                }

                std::thread::sleep(std::time::Duration::from_secs(1));
                if last_generation_instant.elapsed().as_secs()
                    < config.server.profiling.report_interval
                {
                    continue;
                }

                // Update last generation instant
                last_generation_instant = std::time::Instant::now();

                let report = match guard.report().build() {
                    Ok(report) => report,
                    Err(e) => {
                        error!(log, "Failed to generated report: {}", e);
                        break;
                    }
                };

                let time = chrono::Utc::now();
                let time = time.format("%Y-%m-%d-%H-%M-%S").to_string();
                let file_path = config
                    .server
                    .profiling
                    .report_path
                    .join(format!("{}.svg", time));

                let file = match std::fs::File::create(file_path.as_path()) {
                    Ok(file) => file,
                    Err(e) => {
                        error!(log, "Failed to create file to save flamegraph: {}", e);
                        break;
                    }
                };

                if let Err(e) = report.flamegraph(file) {
                    error!(log, "Failed to write flamegraph to file: {}", e);
                }

                // Sweep expired flamegraph files
                if let Err(e) = sweep_expired(&config, &log) {
                    error!(log, "Failed to clean expired flamegraph file: {}", e);
                }
            }
        })
        .unwrap();
}

fn sweep_expired(config: &Arc<Configuration>, log: &Logger) -> std::io::Result<()> {
    let report_path = config.server.profiling.report_path.as_path();
    let mut entries = std::fs::read_dir(report_path)?
        .into_iter()
        .flatten()
        .filter(|entry| {
            let file_name = entry.file_name();
            file_name.as_bytes().ends_with(".svg".as_bytes())
        })
        .collect::<Vec<_>>();

    if entries.len() <= config.server.profiling.max_report_backup {
        return Ok(());
    }

    // Sort by created time in ascending order
    entries.sort_by(|lhs, rhs| {
        let l = match lhs.metadata() {
            Ok(metadata) => match metadata.created() {
                Ok(time) => time,
                Err(_) => {
                    return Ordering::Less;
                }
            },
            Err(_) => {
                return Ordering::Less;
            }
        };

        let r = match rhs.metadata() {
            Ok(metadata) => match metadata.created() {
                Ok(time) => time,
                Err(_) => {
                    return Ordering::Greater;
                }
            },
            Err(_) => {
                return Ordering::Greater;
            }
        };
        l.cmp(&r)
    });

    let mut entries = entries.into_iter().rev().collect::<Vec<_>>();

    loop {
        if entries.len() <= config.server.profiling.max_report_backup {
            break;
        }

        if let Some(entry) = entries.pop() {
            match std::fs::remove_file(entry.path()) {
                Ok(_) => {
                    info!(log, "Removed flamegraph file[{:?}]", entry.path());
                }
                Err(e) => {
                    error!(log, "Failed to remove file[{:?}]: {}", entry.path(), e);
                }
            }
        }
    }
    Ok(())
}
