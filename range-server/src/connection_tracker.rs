use std::{collections::HashMap, net::SocketAddr};

use codec::frame::Frame;
use log::{info, warn};
use protocol::rpc::header::{GoAwayFlags, OperationCode};
use tokio::sync::mpsc::UnboundedSender;

/// Track all existing connections and send the `GoAway` farewell frame to peers if graceful shutdown is desirable.
pub(crate) struct ConnectionTracker {
    /// Map between peer socket address to channel sender.
    ///
    /// Note the reader half of the channel is the connection writer, responsible of writing
    /// frames to remote peer.
    connections: HashMap<SocketAddr, UnboundedSender<Frame>>,
}

impl ConnectionTracker {
    pub(crate) fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    pub(crate) fn insert(&mut self, peer_address: SocketAddr, sender: UnboundedSender<Frame>) {
        self.connections.insert(peer_address, sender);
        info!("Start to track connection to {}", peer_address);
    }

    pub(crate) fn remove(&mut self, peer_address: &SocketAddr) {
        self.connections.remove(peer_address);
        info!("Connection to {} disconnected", peer_address);
    }

    /// Send `GoAway` frame to all existing connections.
    ///
    /// When range server executes scheduled restarts. Graceful shutdown is desirable and
    /// it is a must to disconnect existing long connections gracefully.
    ///
    /// Roughly, three steps are done for this particular purpose:
    /// 1. Stop accepting new connections;
    /// 2. Send `GoAway` frame to each existing connection, requesting them to disconnect as soon as possible;
    /// 3. Await until all connections are terminated after client migrated to other range servers.
    pub(crate) fn go_away(&mut self) {
        self.connections.iter().for_each(|(peer_address, sender)| {
            let mut frame = Frame::new(OperationCode::GOAWAY);
            // Go away frame has stream_id == 0.
            frame.stream_id = 0;
            frame.flag_go_away(GoAwayFlags::SERVER_MAINTENANCE);
            match sender.send(frame) {
                Ok(_) => {
                    info!(
                        "GoAway frame sent to channel bounded to connection={}",
                        peer_address
                    );
                }
                Err(_e) => {
                    warn!(
                        "Failed to send GoAway frame to channel bounded to connection={}",
                        peer_address
                    );
                }
            }
        });
    }

    pub(crate) fn len(&self) -> usize {
        self.connections.len()
    }
}
