//! WhatsApp RPC client — used by CLI commands when the sync daemon is running.

use std::path::Path;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use void_core::models::MessageContent;

use super::path::endpoint_path;
use super::protocol::{
    message_content_to_rpc, RpcDownloadParams, RpcMethod, RpcRequest, RpcResponse, RpcResponseBody,
    RpcResult,
};

static REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_id() -> u64 {
    REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub async fn send_message(
    store_path: &Path,
    connection_id: &str,
    to: &str,
    content: MessageContent,
) -> anyhow::Result<String> {
    let request = RpcRequest {
        id: next_id(),
        connection_id: connection_id.to_string(),
        method: RpcMethod::Send {
            to: to.to_string(),
            content: message_content_to_rpc(&content),
        },
    };
    match call(store_path, request).await? {
        RpcResult::MessageId { message_id } => Ok(message_id),
        other => anyhow::bail!("unexpected RPC result for send: {other:?}"),
    }
}

pub async fn reply_message(
    store_path: &Path,
    connection_id: &str,
    message_id: &str,
    content: MessageContent,
    in_thread: bool,
) -> anyhow::Result<String> {
    let request = RpcRequest {
        id: next_id(),
        connection_id: connection_id.to_string(),
        method: RpcMethod::Reply {
            message_id: message_id.to_string(),
            content: message_content_to_rpc(&content),
            in_thread,
        },
    };
    match call(store_path, request).await? {
        RpcResult::MessageId { message_id } => Ok(message_id),
        other => anyhow::bail!("unexpected RPC result for reply: {other:?}"),
    }
}

pub async fn download_media(
    store_path: &Path,
    connection_id: &str,
    params: RpcDownloadParams,
) -> anyhow::Result<Vec<u8>> {
    let request = RpcRequest {
        id: next_id(),
        connection_id: connection_id.to_string(),
        method: RpcMethod::DownloadMedia { params },
    };
    match call(store_path, request).await? {
        RpcResult::MediaBytes { data_base64 } => STANDARD
            .decode(data_base64)
            .map_err(|e| anyhow::anyhow!("invalid base64 in RPC media response: {e}")),
        other => anyhow::bail!("unexpected RPC result for download: {other:?}"),
    }
}

async fn call(store_path: &Path, request: RpcRequest) -> anyhow::Result<RpcResult> {
    let response = transport_round_trip(store_path, request).await?;
    match response.body {
        RpcResponseBody::Ok { result } => Ok(result),
        RpcResponseBody::Error { error } => anyhow::bail!("{error}"),
    }
}

#[cfg(unix)]
async fn transport_round_trip(
    store_path: &Path,
    request: RpcRequest,
) -> anyhow::Result<RpcResponse> {
    use tokio::net::UnixStream;
    let path = endpoint_path(store_path);
    let mut stream = UnixStream::connect(&path).await.map_err(|e| {
        anyhow::anyhow!(
            "failed to connect to WhatsApp RPC socket at {}: {e}",
            path.display()
        )
    })?;
    stream.write_all(request.encode_line()?.as_bytes()).await?;
    stream.shutdown().await.ok();
    let mut buf = String::new();
    stream.read_to_string(&mut buf).await?;
    RpcResponse::decode_line(&buf)
}

#[cfg(windows)]
async fn transport_round_trip(
    store_path: &Path,
    request: RpcRequest,
) -> anyhow::Result<RpcResponse> {
    use tokio::net::windows::named_pipe::ClientOptions;
    let pipe = endpoint_path(store_path);
    let mut client = ClientOptions::new()
        .open(&pipe)
        .map_err(|e| anyhow::anyhow!("failed to connect to WhatsApp RPC pipe at {pipe}: {e}"))?;
    client.write_all(request.encode_line()?.as_bytes()).await?;
    client.flush().await?;
    let mut buf = String::new();
    client.read_to_string(&mut buf).await?;
    RpcResponse::decode_line(&buf)
}
