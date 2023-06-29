use bytes::Bytes;
use flatbuffers::{FlatBufferBuilder, Verifiable, WIPOffset};
use protocol::rpc::header::{ErrorCode, StatusArgs, SystemError, SystemErrorArgs};

pub(crate) const MIN_BUFFER_SIZE: usize = 64;
pub(crate) const MEDIUM_BUFFER_SIZE: usize = 4 * MIN_BUFFER_SIZE;
#[allow(unused)]
pub(crate) const LARGE_BUFFER_SIZE: usize = 16 * MEDIUM_BUFFER_SIZE;

/// Verifies that a buffer of bytes contains a rpc request and returns it.
pub(crate) fn root_as_rpc_request<'a, R>(buf: &'a [u8]) -> Result<R, flatbuffers::InvalidFlatbuffer>
where
    R: flatbuffers::Follow<'a, Inner = R> + 'a + Verifiable,
{
    flatbuffers::root::<R>(buf)
}

/// Finish the response builder and returns the finished response frame header.
/// Use a generic type to support different response types,
/// and ensure the finished_data is called after the builder.finish().
pub(crate) fn finish_response_builder<R>(
    builder: &mut FlatBufferBuilder,
    response_offset: WIPOffset<R>,
) -> Bytes {
    builder.finish(response_offset, None);
    Bytes::copy_from_slice(builder.finished_data())
}

pub(crate) fn system_error_frame_bytes(err_code: ErrorCode, err_msg: &str) -> Bytes {
    let mut builder = FlatBufferBuilder::with_capacity(MIN_BUFFER_SIZE);

    let error_msg = builder.create_string(err_msg);
    let status = protocol::rpc::header::Status::create(
        &mut builder,
        &StatusArgs {
            code: err_code,
            message: Some(error_msg),
            detail: None,
        },
    );
    let res_args = SystemErrorArgs {
        status: Some(status),
    };

    let response_offset = SystemError::create(&mut builder, &res_args);
    finish_response_builder(&mut builder, response_offset)
}
