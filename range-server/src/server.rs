use crate::{
    stream_manager::{
        fetcher::{DelegatePlacementClient, PlacementClient},
        manager::DefaultStreamManager,
    },
    worker::Worker,
    worker_config::WorkerConfig,
};
use config::Configuration;
use log::{error, info};
use std::{cell::UnsafeCell, error::Error, os::fd::AsRawFd, rc::Rc, sync::Arc, thread};
use store::{ElasticStore, Store};

use tokio::sync::{broadcast, mpsc, oneshot};

pub fn launch(
    config: Configuration,
    shutdown: broadcast::Sender<()>,
) -> Result<(), Box<dyn Error>> {
    let (recovery_completion_tx, recovery_completion_rx) = oneshot::channel();

    // Note we move the configuration into store, letting it either allocate or read existing server-id for us.
    let store = match ElasticStore::new(config, recovery_completion_tx) {
        Ok(store) => store,
        Err(e) => {
            error!("Failed to launch ElasticStore: {:?}", e);
            return Err(Box::new(e));
        }
    };

    // Acquire a shared ref to full-fledged configuration.
    let config = store.config();

    if config.server.profiling.enable {
        crate::profiling::generate_flame_graph(Arc::clone(&config), shutdown.subscribe());
        info!("Continuous profiling starts");
    }

    recovery_completion_rx.blocking_recv()?;

    let mut channels = vec![];

    let worker_core_ids = config::parse_cpu_set(&config.server.worker_cpu_set);
    debug_assert!(
        !worker_core_ids.is_empty(),
        "At least one core should be reserved for primary worker"
    );

    // Build non-primary workers first
    let mut handles = worker_core_ids
        .iter()
        .rev()
        .skip(1)
        .map(|id| core_affinity::CoreId { id: *id as usize })
        .map(|core_id| {
            let server_config = config.clone();
            let store = store.clone();
            let (tx, rx) = mpsc::unbounded_channel();
            channels.push(rx);

            let shutdown_tx = shutdown.clone();
            thread::Builder::new()
                .name("RangeServer".to_owned())
                .spawn(move || {
                    let worker_config = WorkerConfig {
                        core_id,
                        server_config: Arc::clone(&server_config),
                        sharing_uring: store.as_raw_fd(),
                        primary: false,
                    };

                    let store = Rc::new(store);
                    let client = Rc::new(client::Client::new(
                        Arc::clone(&server_config),
                        shutdown_tx.clone(),
                    ));

                    let fetcher = DelegatePlacementClient::new(tx);
                    let stream_manager = Rc::new(UnsafeCell::new(DefaultStreamManager::new(
                        fetcher,
                        Rc::clone(&store),
                    )));
                    let mut worker =
                        Worker::new(worker_config, store, stream_manager, client, None);
                    worker.serve(shutdown_tx)
                })
        })
        .collect::<Vec<_>>();

    // Build primary worker
    {
        let core_id = worker_core_ids
            .iter()
            .rev()
            .map(|id| core_affinity::CoreId { id: *id as usize })
            .next()
            .unwrap();
        let server_config = config;
        let shutdown_tx = shutdown;
        let handle = thread::Builder::new()
            .name("RangeServer[Primary]".to_owned())
            .spawn(move || {
                let worker_config = WorkerConfig {
                    core_id,
                    server_config: Arc::clone(&server_config),
                    sharing_uring: store.as_raw_fd(),
                    primary: true,
                };

                let client = Rc::new(client::Client::new(
                    Arc::clone(&server_config),
                    shutdown_tx.clone(),
                ));
                let fetcher = PlacementClient::new(Rc::clone(&client));
                let store = Rc::new(store);

                let stream_manager = Rc::new(UnsafeCell::new(DefaultStreamManager::new(
                    fetcher,
                    Rc::clone(&store),
                )));

                let mut worker =
                    Worker::new(worker_config, store, stream_manager, client, Some(channels));
                worker.serve(shutdown_tx)
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

    use log::info;

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
                            info!(
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
