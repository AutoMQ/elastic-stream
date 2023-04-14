use crate::{
    stream_manager::{fetcher::Fetcher, StreamManager},
    worker::Worker,
    worker_config::WorkerConfig,
};
use config::Configuration;
use slog::{error, o, warn, Drain, Logger};
use slog_async::Async;
use slog_term::{FullFormat, TermDecorator};
use std::{cell::RefCell, error::Error, os::fd::AsRawFd, rc::Rc, sync::Arc, thread};
use store::ElasticStore;
use tokio::sync::{broadcast, mpsc, oneshot};

pub fn launch(config: Configuration, shutdown: broadcast::Sender<()>) -> Result<(), Box<dyn Error>> {
    let decorator = TermDecorator::new().build();
    let drain = FullFormat::new(decorator)
        .use_file_location()
        .build()
        .fuse();
    let drain = Async::new(drain).build().fuse();
    let log = Logger::root(drain, o!());

    let core_ids = core_affinity::get_core_ids().ok_or_else(|| {
        warn!(log, "No cores are available to set affinity");
        crate::error::LaunchError::NoCoresAvailable
    })?;
    let available_core_len = core_ids.len();

    let (recovery_completion_tx, recovery_completion_rx) = oneshot::channel();

    // Note we move the configuration into store, letting it either allocate or read existing node-id for us.
    let store = match ElasticStore::new(log.clone(), config, recovery_completion_tx) {
        Ok(store) => store,
        Err(e) => {
            error!(log, "Failed to launch ElasticStore: {:?}", e);
            return Err(Box::new(e));
        }
    };

    // Acquire a shared ref to full-fledged configuration.
    let config = store.config();

    recovery_completion_rx.blocking_recv()?;

    let mut channels = vec![];

    // Build non-primary workers first
    let mut handles = core_ids
        .iter()
        .skip(available_core_len - config.server.concurrency + 1)
        .map(|core_id| {
            let server_config = config.clone();
            let logger = log.new(o!());
            let store = store.clone();
            let core_id = core_id.clone();
            let (tx, rx) = mpsc::unbounded_channel();
            channels.push(rx);

            let shutdown_rx = shutdown.subscribe();
            thread::Builder::new()
                .name("DataNode".to_owned())
                .spawn(move || {
                    let worker_config = WorkerConfig {
                        core_id,
                        server_config,
                        sharing_uring: store.as_raw_fd(),
                        primary: false,
                    };

                    let store = Rc::new(store);

                    let fetcher = Fetcher::Channel { sender: tx };
                    let stream_manager = Rc::new(RefCell::new(StreamManager::new(
                        logger.clone(),
                        fetcher,
                        Rc::clone(&store),
                    )));
                    let mut worker =
                        Worker::new(worker_config, store, stream_manager, None, &logger);
                    worker.serve(shutdown_rx)
                })
        })
        .collect::<Vec<_>>();

    // Build primary worker
    {
        let core_id = core_ids
            .get(available_core_len - config.server.concurrency)
            .expect("At least one core should be reserved for primary node")
            .clone();
        let server_config = config.clone();
        let shutdown_rx = shutdown.subscribe();
        let handle = thread::Builder::new()
            .name("DataNode[Primary]".to_owned())
            .spawn(move || {
                let worker_config = WorkerConfig {
                    core_id,
                    server_config: Arc::clone(&server_config),
                    sharing_uring: store.as_raw_fd(),
                    primary: true,
                };

                let placement_client = client::Client::new(Arc::clone(&server_config), &log);

                let fetcher = Fetcher::PlacementClient {
                    client: placement_client,
                    target: worker_config.server_config.server.placement_manager.clone(),
                };
                let store = Rc::new(store);

                let stream_manager = Rc::new(RefCell::new(StreamManager::new(
                    log.clone(),
                    fetcher,
                    Rc::clone(&store),
                )));

                let mut worker =
                    Worker::new(worker_config, store, stream_manager, Some(channels), &log);
                worker.serve(shutdown_rx)
            });
        handles.push(handle);
    }

    for handle in handles.into_iter() {
        let _result = handle.unwrap().join();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cmp::min;

    #[test]
    fn test_core_affinity() {
        let core_ids = core_affinity::get_core_ids().unwrap();
        let core_count = min(core_ids.len(), 2);

        let len = core_ids.len();
        let handles = core_ids
            .into_iter()
            .skip(len - core_count)
            .map(|processor_id| {
                std::thread::Builder::new()
                    .name("Worker".into())
                    .spawn(move || {
                        if core_affinity::set_for_current(processor_id) {
                            println!(
                                "Set affinity for worker thread {:?} OK",
                                std::thread::current()
                            );
                        }
                    })
            })
            .collect::<Vec<_>>();

        for handle in handles.into_iter() {
            handle.unwrap().join().unwrap();
        }
    }
}
