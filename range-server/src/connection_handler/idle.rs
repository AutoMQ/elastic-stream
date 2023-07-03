use std::{
    cell::RefCell,
    net::SocketAddr,
    rc::{Rc, Weak},
    sync::Arc,
    time::{Duration, Instant},
};

use config::Configuration;
use log::info;
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
    ) -> Rc<Self> {
        let handler = Rc::new(Self {
            config: Arc::clone(&config),
            last_read: Rc::new(RefCell::new(Instant::now())),
            last_write: Rc::new(RefCell::new(Instant::now())),
        });
        Self::run(Rc::clone(&handler), connection, addr, config, conn_tracker);
        handler
    }

    pub(crate) fn on_read(&self) {
        *self.last_read.borrow_mut() = Instant::now();
    }

    pub(crate) fn on_write(&self) {
        *self.last_write.borrow_mut() = Instant::now();
    }

    fn read_idle(&self) -> bool {
        *self.last_read.borrow() + self.config.connection_idle_duration() < Instant::now()
    }

    fn write_idle(&self) -> bool {
        *self.last_write.borrow() + self.config.connection_idle_duration() < Instant::now()
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
    ) {
        tokio_uring::spawn(async move {
            let mut interval = tokio::time::interval(config.connection_idle_duration());
            loop {
                interval.tick().await;
                match connection.upgrade() {
                    Some(channel) => {
                        if handler.read_idle() && handler.write_idle() {
                            conn_tracker.borrow_mut().remove(&addr);
                            if channel.close().is_ok() {
                                info!(
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

#[cfg(test)]
mod tests {
    use crate::connection_tracker::ConnectionTracker;
    use crate::mocks::run_listener;
    use config::Configuration;
    use std::{cell::RefCell, error::Error, net::SocketAddr, rc::Rc, sync::Arc};
    use tokio_uring::net::TcpStream;
    use transport::connection::Connection;

    #[test]
    fn test_read_idle() -> Result<(), Box<dyn Error>> {
        let mut config = Configuration::default();
        config.server.connection_idle_duration = 1;
        let config = Arc::new(config);
        tokio_uring::start(async {
            let port = run_listener().await;
            let target = format!("127.0.0.1:{}", port);
            let addr = target.parse::<SocketAddr>().unwrap();
            let stream = TcpStream::connect(addr.clone()).await.unwrap();
            let connection = Rc::new(Connection::new(stream, &target));
            let conn_tracker = Rc::new(RefCell::new(ConnectionTracker::new()));
            let handler = super::IdleHandler::new(
                Rc::downgrade(&connection),
                addr,
                Arc::clone(&config),
                conn_tracker,
            );
            tokio::time::sleep(config.connection_idle_duration()).await;
            assert!(handler.read_idle(), "Read should be idle");
            assert!(handler.write_idle(), "Write should be idle");

            handler.on_read();
            assert_eq!(false, handler.read_idle(), "Read should NOT be idle");
            assert!(handler.write_idle(), "Write should be idle");

            handler.on_write();
            assert_eq!(false, handler.write_idle(), "Read should NOT be idle");

            Ok(())
        })
    }
}
