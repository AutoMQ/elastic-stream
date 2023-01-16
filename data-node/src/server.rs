use std::{error::Error, rc::Rc};

use crate::{cfg::ServerConfig, handler::ServerCall};

use core_affinity::CoreId;
use monoio::{
    io::Splitable,
    net::{TcpListener, TcpStream},
    FusionDriver, RuntimeBuilder,
};
use slog::{debug, error, info, o, warn, Drain, Logger};
use slog_async::Async;
use slog_term::{CompactFormat, TermDecorator};
use store::{
    elastic::ElasticStore,
    option::{StoreOptions, StorePath},
};
use transport::channel::{ChannelReader, ChannelWriter};

struct NodeConfig {
    core_id: CoreId,
    server_config: ServerConfig,
}

struct Node {
    config: NodeConfig,
    store: Option<Rc<ElasticStore>>,
    logger: Logger,
}

impl Node {
    pub fn new(config: NodeConfig, logger: &Logger) -> Self {
        Self {
            config,
            store: None,
            logger: logger.clone(),
        }
    }

    pub fn serve(&mut self) {
        monoio::utils::bind_to_cpu_set([self.config.core_id.id]).unwrap();
        let mut driver = match RuntimeBuilder::<FusionDriver>::new()
            .enable_timer()
            .with_entries(self.config.server_config.queue_depth)
            .build()
        {
            Ok(driver) => driver,
            Err(e) => {
                error!(
                    self.logger,
                    "Failed to create runtime. Cause: {}",
                    e.to_string()
                );
                panic!("Failed to create runtime driver. {}", e.to_string());
            }
        };

        driver.block_on(async {
            let store_path = StorePath::new("/tmp", 0);
            let store_options = StoreOptions::new(&store_path);
            let store = match ElasticStore::new(&store_options, &self.logger) {
                Ok(store) => store,
                Err(e) => {
                    error!(self.logger, "Failed to create ElasticStore. Cause: {:?}", e);
                    panic!("Failed to create ElasticStore");
                }
            };

            match store.open().await {
                Ok(_) => {}
                Err(e) => {
                    error!(self.logger, "Failed to initiaize store. Cause: {:?}", e);
                }
            };

            self.store = Some(store);

            let bind_address = format!("0.0.0.0:{}", self.config.server_config.port);
            let listener = match TcpListener::bind(&bind_address) {
                Ok(listener) => {
                    info!(self.logger, "Server starts OK, listening {}", bind_address);
                    listener
                }
                Err(e) => {
                    eprintln!("{}", e.to_string());
                    return;
                }
            };

            match self.run(listener, self.logger.new(o!())).await {
                Ok(_) => {}
                Err(e) => {
                    error!(self.logger, "Runtime failed. Cause: {}", e.to_string());
                }
            }
        });
    }

    async fn run(&self, listener: TcpListener, logger: Logger) -> Result<(), Box<dyn Error>> {
        loop {
            let incoming = listener.accept().await;
            let logger = logger.new(o!());
            let (stream, _socket_address) = match incoming {
                Ok((stream, socket_addr)) => {
                    debug!(logger, "Accepted a new connection from {socket_addr:?}");
                    stream.set_nodelay(true).unwrap_or_else(|e| {
                        warn!(logger, "Failed to disable Nagle's algorithm. Cause: {e:?}, PeerAddress: {socket_addr:?}");
                    });
                    debug!(logger, "Nagle's algorithm turned off");

                    (stream, socket_addr)
                }
                Err(e) => {
                    error!(
                        logger,
                        "Failed to accept a connection. Cause: {}",
                        e.to_string()
                    );
                    break;
                }
            };

            if let Some(ref store) = self.store {
                let store = Rc::clone(store);
                monoio::spawn(async move {
                    Node::process(store, stream, logger).await;
                });
            } else {
                error!(self.logger, "Store is not properly initilialized");
                break;
            }
        }

        Ok(())
    }

    async fn process(store: Rc<ElasticStore>, stream: TcpStream, logger: Logger) {
        let peer_address = match stream.peer_addr() {
            Ok(addr) => addr.to_string(),
            Err(_e) => "Unknown".to_owned(),
        };

        let (read_half, write_half) = stream.into_split();
        let mut channel_reader = ChannelReader::new(read_half, &peer_address, logger.new(o!()));
        let mut channel_writer = ChannelWriter::new(write_half, &peer_address, logger.new(o!()));
        let (tx, rx) = async_channel::unbounded();

        let request_logger = logger.clone();
        monoio::spawn(async move {
            let logger = request_logger;
            loop {
                match channel_reader.read_frame().await {
                    Ok(Some(frame)) => {
                        let log = logger.new(o!());
                        let sender = tx.clone();
                        let store = Rc::clone(&store);
                        let mut server_call = ServerCall {
                            request: frame,
                            sender,
                            logger: log,
                            store,
                        };
                        monoio::spawn(async move {
                            server_call.call().await;
                        });
                    }
                    Ok(None) => {
                        info!(logger, "Connection to {} is closed", peer_address);
                        break;
                    }
                    Err(e) => {
                        warn!(
                            logger,
                            "Connection reset. Peer address: {}. Cause: {e:?}", peer_address
                        );
                        break;
                    }
                }
            }
        });

        monoio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(frame) => match channel_writer.write_frame(&frame).await {
                        Ok(_) => {
                            debug!(
                                logger,
                                "Response[stream-id={:?}] written to network", frame.stream_id
                            );
                        }
                        Err(e) => {
                            warn!(
                                logger,
                                "Failed to write response[stream-id={:?}] to network. Cause: {:?}",
                                frame.stream_id,
                                e
                            );
                            break;
                        }
                    },
                    Err(e) => {
                        warn!(
                            logger,
                            "Failed to receive response frame from MSPC channel. Cause: {:?}", e
                        );
                        break;
                    }
                }
            }
        });
    }
}

pub fn launch(cfg: &ServerConfig) {
    let decorator = TermDecorator::new().build();
    let drain = CompactFormat::new(decorator).build().fuse();
    let drain = Async::new(drain).build().fuse();
    let log = Logger::root(drain, o!());

    let core_ids = match core_affinity::get_core_ids() {
        Some(ids) => ids,
        None => {
            warn!(log, "No cores are available to set affinity");
            return;
        }
    };
    let available_core_len = core_ids.len();

    let handles = core_ids
        .into_iter()
        .skip(available_core_len - cfg.concurrency)
        .map(|core_id| {
            let server_config = cfg.clone();
            let logger = log.new(o!());

            std::thread::Builder::new()
                .name("Worker".to_owned())
                .spawn(move || {
                    let node_config = NodeConfig {
                        core_id: core_id.clone(),
                        server_config: server_config.clone(),
                    };
                    let mut node = Node::new(node_config, &logger);
                    node.serve()
                })
        })
        .collect::<Vec<_>>();

    for handle in handles.into_iter() {
        let _result = handle.unwrap().join();
    }
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
