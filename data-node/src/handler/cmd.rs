use std::{cell::RefCell, rc::Rc};

use codec::frame::{Frame, OperationCode};
use protocol::rpc::header::ErrorCode;
use store::ElasticStore;

use crate::stream_manager::StreamManager;

use super::{
    append::Append, fetch::Fetch, heartbeat::Heartbeat, ping::Ping,
    seal_range::SealRange,
};

#[derive(Debug)]
pub(crate) enum Command<'a> {
    Append(Append<'a>),
    Fetch(Fetch<'a>),
    SealRange(SealRange<'a>),
    Ping(Ping<'a>),
    Heartbeat(Heartbeat<'a>),
}

impl<'a> Command<'a> {
    pub fn from_frame(frame: &Frame) -> Result<Command, ErrorCode> {
        match frame.operation_code {
            OperationCode::Unknown => todo!(),

            OperationCode::Ping => Ok(Command::Ping(Ping::new(frame))),

            OperationCode::GoAway => todo!(),

            OperationCode::Heartbeat => Ok(Command::Heartbeat(Heartbeat::parse_frame(frame)?)),

            OperationCode::AllocateId => unreachable!(),

            OperationCode::Append => Ok(Command::Append(Append::parse_frame(frame)?)),

            OperationCode::Fetch => Ok(Command::Fetch(Fetch::parse_frame(frame)?)),

            OperationCode::ListRange => todo!(),

            OperationCode::SealRange => Ok(Command::SealRange(SealRange::parse_frame(frame)?)),

            OperationCode::SyncRange => todo!(),
            OperationCode::CreateStream => todo!(),
            OperationCode::DeleteStream => todo!(),
            OperationCode::UpdateStream => todo!(),
            OperationCode::DescribeStream => todo!(),
            OperationCode::TrimStream => todo!(),
            OperationCode::ReportMetrics => todo!(),
            OperationCode::DescribePlacementManager => {
                todo!()
            }
        }
    }

    pub(crate) async fn apply(
        &self,
        store: Rc<ElasticStore>,
        stream_manager: Rc<RefCell<StreamManager>>,
        response: &mut Frame,
    ) {
        match self {
            Command::Append(cmd) => cmd.apply(store, stream_manager, response).await,
            Command::Fetch(cmd) => cmd.apply(store, stream_manager, response).await,
            Command::Heartbeat(cmd) => cmd.apply(store, stream_manager, response).await,
            Command::Ping(cmd) => cmd.apply(store, stream_manager, response).await,
            Command::SealRange(cmd) => cmd.apply(store, stream_manager, response).await,
        }
    }
}
