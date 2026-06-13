//! IPC for WhatsApp send/reply/download through the sync daemon connection.

mod client;
mod path;
mod protocol;
mod server;

#[cfg(test)]
mod tests;

pub use client::{download_media, reply_message, send_message};
pub use protocol::{
    message_content_to_rpc, rpc_to_message_content, RpcContent, RpcDownloadParams, RpcMethod,
    RpcRequest, RpcResponse, RpcResult,
};
pub use server::Server;
