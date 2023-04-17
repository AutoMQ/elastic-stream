//! Server-side handlers, processors for requests of each kind.
//!
//! See details docs for each operation code

mod append;
mod cmd;
mod describe_range;
mod fetch;
mod go_away;
mod heartbeat;
mod ping;
mod seal_range;
mod util;
use self::cmd::Command;
use crate::{
    metrics::{APPEND_LATENCY, FETCH_LATENCY},
    stream_manager::StreamManager,
};
use codec::frame::Frame;
use slog::{trace, warn, Logger};
use std::{cell::RefCell, rc::Rc};
use store::ElasticStore;

/// Representation of the incoming request.
///
///
pub struct ServerCall {
    /// The incoming request
    pub(crate) request: Frame,

    /// Sender for future response
    ///
    /// Note the receiver part is polled by `ChannelWriter` in a spawned task.
    pub(crate) sender: tokio::sync::mpsc::UnboundedSender<Frame>,

    /// Logger
    pub(crate) logger: Logger,

    /// `Store` to query, persist and replicate records.
    ///
    /// Note this store is `!Send` as it follows thread-per-core pattern.
    pub(crate) store: Rc<ElasticStore>,

    pub(crate) stream_manager: Rc<RefCell<StreamManager>>,
}

impl ServerCall {
    /// Serve the incoming request
    ///
    /// Delegate each incoming request to its dedicated `on_xxx` method according to
    /// operation code.
    pub async fn call(&mut self) {
        trace!(
            self.logger,
            "Receive a request. stream-id={}, opcode={}",
            self.request.stream_id,
            self.request.operation_code
        );
        let now = std::time::Instant::now();
        let mut response = Frame::new(self.request.operation_code);
        response.stream_id = self.request.stream_id;

        // Flag it's a response frame, as well as the end of the response.
        // If the response sequence is not ended, please note reset the flag in the subsequent logic.
        response.flag_end_of_response_stream();

        let cmd = match Command::from_frame(self.logger.clone(), &self.request).ok() {
            Some(it) => it,
            None => {
                // TODO: return error response
                return;
            }
        };

        // Log the `cmd` object.
        trace!(
            self.logger,
            "Command of frame[stream-id={}]: {:?}",
            self.request.stream_id,
            cmd
        );

        // Delegate the request to its dedicated handler.
        cmd.apply(
            Rc::clone(&self.store),
            Rc::clone(&self.stream_manager),
            &mut response,
        )
        .await;
        match cmd {
            Command::Append(_) => {
                APPEND_LATENCY.observe(now.elapsed().as_micros() as f64);
            }
            Command::Fetch(_) => {
                FETCH_LATENCY.observe(now.elapsed().as_micros() as f64);
            }
            _ => {}
        }
        trace!(
            self.logger,
            "Response frame generated. stream-id={}",
            response.stream_id
        );

        // Send response to channel.
        // Note there is a spawned task, in which channel writer is polling the channel.
        // Once the response is received, it would immediately get written to network.
        match self.sender.send(response) {
            Ok(_) => {
                trace!(
                    self.logger,
                    "Response[stream-id={}] transferred to channel",
                    self.request.stream_id
                );
            }
            Err(e) => {
                warn!(
                    self.logger,
                    "Failed to send response[stream-id={}] to channel. Cause: {:?}",
                    self.request.stream_id,
                    e
                );
            }
        };
    }
}
