use std::{
    cell::RefCell,
    net::SocketAddr,
    rc::{Rc, Weak},
    sync::Arc,
    time::{Duration, Instant},
};

use config::Configuration;
use slog::{info, Logger};
use transport::connection::Connection;

use crate::connection_tracker::ConnectionTracker;

pub(crate) struct IdleHandler {
    config: Arc<Configuration>,
    last_read: Rc<RefCell<Instant>>,
    last_write: Rc<RefCell<Instant>>,
}

impl IdleHandler {
    pub(crate) fn new(
        connection: Weak<Connection>,
        addr: SocketAddr,
        config: Arc<Configuration>,
        conn_tracker: Rc<RefCell<ConnectionTracker>>,
        log: Logger,
    ) -> Rc<Self> {
        let handler = Rc::new(Self {
            config: Arc::clone(&config),
            last_read: Rc::new(RefCell::new(Instant::now())),
            last_write: Rc::new(RefCell::new(Instant::now())),
        });
        Self::run(
            Rc::clone(&handler),
            connection,
            addr,
            config,
            conn_tracker,
            log,
        );
        handler
    }

    pub(crate) fn on_read(&self) {
        *self.last_read.borrow_mut() = Instant::now();
    }

    pub(crate) fn on_write(&self) {
        *self.last_write.borrow_mut() = Instant::now();
    }

    fn read_idle(&self) -> bool {
        self.last_read.borrow().clone() + self.config.connection_idle_duration() < Instant::now()
    }

    fn write_idle(&self) -> bool {
        self.last_write.borrow().clone() + self.config.connection_idle_duration() < Instant::now()
    }

    fn read_elapsed(&self) -> Duration {
        self.last_read.borrow().elapsed()
    }

    fn write_elapsed(&self) -> Duration {
        self.last_write.borrow().elapsed()
    }

    fn run(
        handler: Rc<Self>,
        connection: Weak<Connection>,
        addr: SocketAddr,
        config: Arc<Configuration>,
        conn_tracker: Rc<RefCell<ConnectionTracker>>,
        log: Logger,
    ) {
        tokio_uring::spawn(async move {
            let mut interval = tokio::time::interval(config.connection_idle_duration());
            loop {
                interval.tick().await;
                match connection.upgrade() {
                    Some(channel) => {
                        if handler.read_idle() && handler.write_idle() {
                            conn_tracker.borrow_mut().remove(&addr);
                            if let Ok(_) = channel.close() {
                                info!(
                                    log,
                                    "Close connection to {} since read has been idle for {}ms and write has been idle for {}ms",
                                    channel.peer_address(),
                                    handler.read_elapsed().as_millis(),
                                    handler.write_elapsed().as_millis(),
                                );
                            }
                        }
                    }
                    None => {
                        // If the connection was already destructed, stop idle checking automatically.
                        break;
                    }
                }
            }
        });
    }
}
