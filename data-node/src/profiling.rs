use std::{cmp::Ordering, os::unix::prelude::OsStrExt, path::Path, sync::Arc};

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
            let cwd = match std::env::current_dir() {
                Ok(path) => path,
                Err(e) => {
                    error!(log, "Failed to acquire current working directory: {}", e);
                    return;
                }
            };

            let base_path = cwd.join(&config.server.profiling.report_path);
            if !base_path.exists() {
                if let Err(e) = std::fs::create_dir_all(base_path.as_path()) {
                    error!(
                        log,
                        "Failed to create directory[{:?}] to save flamegraph files", base_path
                    )
                }
            }

            loop {
                match shutdown.try_recv() {
                    Err(TryRecvError::Empty) => {}
                    _ => {
                        info!(log, "Continuous Profiling stopped");
                        break;
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
                let file_path = base_path.join(format!("{}.svg", time));
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
                if let Err(e) = sweep_expired(base_path.as_path(), &config, &log) {
                    error!(log, "Failed to clean expired flamegraph file: {}", e);
                }
            }
        })
        .unwrap();
}

fn sweep_expired(
    report_path: &Path,
    config: &Arc<Configuration>,
    log: &Logger,
) -> std::io::Result<()> {
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
