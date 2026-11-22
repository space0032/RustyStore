//! Helpers for outgoing gRPC calls.

/// Wrap a message in a `tonic::Request`, attaching the request id scoped in
/// `common::request_id` as metadata when one is present.
///
/// This lets the coordinator and storage nodes correlate their logs with the
/// originating HTTP request without every call site handling metadata.
pub(crate) fn request_with_id<T>(message: T) -> tonic::Request<T> {
    let mut request = tonic::Request::new(message);
    if let Some(id) = common::request_id::current()
        && let Ok(value) = id.parse::<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>()
    {
        request
            .metadata_mut()
            .insert(common::request_id::HEADER, value);
    }
    request
}
