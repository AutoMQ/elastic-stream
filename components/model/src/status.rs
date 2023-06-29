use bytes::Bytes;
use protocol::rpc::header::{ErrorCode, StatusT};

#[derive(Debug, Clone)]
pub struct Status {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<Bytes>,
}

impl From<&StatusT> for Status {
    fn from(value: &StatusT) -> Self {
        Self {
            code: value.code,
            message: value.message.clone().unwrap_or_default(),
            details: value.detail.clone().map(Into::into),
        }
    }
}

impl From<&Box<StatusT>> for Status {
    fn from(value: &Box<StatusT>) -> Self {
        Self {
            code: value.code,
            message: value.message.clone().unwrap_or_default(),
            details: value.detail.clone().map(Into::into),
        }
    }
}

impl Status {
    pub fn ok() -> Self {
        Self {
            code: ErrorCode::OK,
            message: "OK".to_owned(),
            details: None,
        }
    }

    pub fn unspecified() -> Self {
        Self {
            code: ErrorCode::ERROR_CODE_UNSPECIFIED,
            message: "Bad response, missing status".to_owned(),
            details: None,
        }
    }

    pub fn decode() -> Self {
        Self {
            code: ErrorCode::DECODE,
            message: "Decoding frame header failure".to_owned(),
            details: None,
        }
    }

    pub fn pd_internal(message: String) -> Self {
        Self {
            code: ErrorCode::PD_INTERNAL_SERVER_ERROR,
            message,
            details: None,
        }
    }

    pub fn dn_internal(message: String) -> Self {
        Self {
            code: ErrorCode::RS_INTERNAL_SERVER_ERROR,
            message,
            details: None,
        }
    }

    pub fn bad_request(message: String) -> Self {
        Self {
            code: ErrorCode::BAD_REQUEST,
            message,
            details: None,
        }
    }
}
