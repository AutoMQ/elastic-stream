use std::{cell::UnsafeCell, fmt, rc::Rc};

use codec::frame::{Frame, OperationCode};
use log::error;
use protocol::rpc::header::ErrorCode;
use store::Store;

use crate::stream_manager::StreamManager;

use super::{
    append::Append, create_range::CreateRange, fetch::Fetch, heartbeat::Heartbeat, ping::Ping,
    seal_range::SealRange,
};

#[derive(Debug)]
pub(crate) enum Command<'a> {
    Append(Append),
    Fetch(Fetch<'a>),
    CreateRange(CreateRange<'a>),
    SealRange(SealRange<'a>),
    Ping(Ping<'a>),
    Heartbeat(Heartbeat<'a>),
}

impl<'a> Command<'a> {
    pub fn from_frame(frame: &Frame) -> Result<Command, ErrorCode> {
        match frame.operation_code {
            OperationCode::Unknown => Err(ErrorCode::UNSUPPORTED_OPERATION),

            OperationCode::Ping => Ok(Command::Ping(Ping::new(frame))),

            OperationCode::GoAway => {
                error!("GoAway is not supported in range-server");
                Err(ErrorCode::UNSUPPORTED_OPERATION)
            }

            OperationCode::Heartbeat => Ok(Command::Heartbeat(Heartbeat::parse_frame(frame)?)),

            OperationCode::AllocateId => {
                error!("AllocateId is not supported in range-server");
                Err(ErrorCode::UNSUPPORTED_OPERATION)
            }

            OperationCode::Append => Ok(Command::Append(Append::parse_frame(frame)?)),

            OperationCode::Fetch => Ok(Command::Fetch(Fetch::parse_frame(frame)?)),

            OperationCode::ListRange => {
                error!("ListRange is not supported in range-server");
                Err(ErrorCode::UNSUPPORTED_OPERATION)
            }

            OperationCode::CreateRange => {
                Ok(Command::CreateRange(CreateRange::parse_frame(frame)?))
            }

            OperationCode::SealRange => Ok(Command::SealRange(SealRange::parse_frame(frame)?)),

            OperationCode::SyncRange => Err(ErrorCode::UNSUPPORTED_OPERATION),

            OperationCode::CreateStream => {
                error!("CreateStream is not supported in range-server");
                Err(ErrorCode::UNSUPPORTED_OPERATION)
            }

            OperationCode::DeleteStream => {
                error!("DeleteStream is not supported in range-server");
                Err(ErrorCode::UNSUPPORTED_OPERATION)
            }

            OperationCode::UpdateStream => {
                error!("UpdateStream is not supported in range-server");
                Err(ErrorCode::UNSUPPORTED_OPERATION)
            }

            OperationCode::DescribeStream => {
                error!("DescribeStream is not supported in range-server");
                Err(ErrorCode::UNSUPPORTED_OPERATION)
            }
            OperationCode::TrimStream => {
                error!("TrimStream is not supported in range-server");
                Err(ErrorCode::UNSUPPORTED_OPERATION)
            }

            OperationCode::ReportMetrics => {
                error!("ReportMetrics is not supported in range-server");
                Err(ErrorCode::UNSUPPORTED_OPERATION)
            }

            OperationCode::DescribePlacementDriver => {
                error!("DescribePlacementDriver is not supported in range-server");
                Err(ErrorCode::UNSUPPORTED_OPERATION)
            }
        }
    }

    pub(crate) async fn apply<S, M>(
        &self,
        store: Rc<S>,
        stream_manager: Rc<UnsafeCell<M>>,
        response: &mut Frame,
    ) where
        S: Store,
        M: StreamManager,
    {
        match self {
            Command::Append(cmd) => cmd.apply(store, stream_manager, response).await,
            Command::Fetch(cmd) => cmd.apply(store, stream_manager, response).await,
            Command::Heartbeat(cmd) => cmd.apply(store, stream_manager, response).await,
            Command::Ping(cmd) => cmd.apply(store, stream_manager, response).await,
            Command::CreateRange(cmd) => cmd.apply(store, stream_manager, response).await,
            Command::SealRange(cmd) => cmd.apply(store, stream_manager, response).await,
        }
    }
}

impl<'a> fmt::Display for Command<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Append(cmd) => write!(f, "{}", cmd),
            Command::Fetch(cmd) => write!(f, "{}", cmd),
            Command::Heartbeat(cmd) => write!(f, "{}", cmd),
            Command::Ping(cmd) => write!(f, "{}", cmd),
            Command::CreateRange(cmd) => write!(f, "{}", cmd),
            Command::SealRange(cmd) => write!(f, "{}", cmd),
        }
    }
}
