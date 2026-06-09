use crate::fixtures::VALIDATOR_ID;
use detta_core::{DeTTaState, ValidatorNode};
use detta_network::{NetworkMessage, TcpProtocolStream};
use detta_node::PersistentValidatorNode;
use detta_rpc::{HttpJsonRpcServer, JsonRpcServer, RpcService, RpcTransportError};
use std::net::{SocketAddr, TcpListener};
use std::thread::{self, JoinHandle};

pub struct RpcServerHandle {
    join: Option<JoinHandle<Result<(), String>>>,
}

impl RpcServerHandle {
    pub fn join(mut self) -> Result<(), String> {
        let join = self
            .join
            .take()
            .expect("RPC server handle was already joined");
        match join.join() {
            Ok(result) => result,
            Err(_) => Err("RPC server thread panicked".into()),
        }
    }
}

pub fn spawn_tcp_rpc_server(state: DeTTaState) -> Result<(SocketAddr, RpcServerHandle), String> {
    let server = JsonRpcServer::bind("127.0.0.1:0").map_err(format_transport_error)?;
    let addr = server.local_addr().map_err(format_transport_error)?;
    let join = thread::spawn(move || {
        let mut rpc = RpcService::new(ValidatorNode::new(VALIDATOR_ID, state));
        server
            .serve_next_connection(&mut rpc)
            .map_err(format_transport_error)
    });
    Ok((addr, RpcServerHandle { join: Some(join) }))
}

pub fn spawn_http_rpc_server(
    state: DeTTaState,
    request_count: usize,
) -> Result<(SocketAddr, RpcServerHandle), String> {
    let server = HttpJsonRpcServer::bind("127.0.0.1:0").map_err(format_transport_error)?;
    let addr = server.local_addr().map_err(format_transport_error)?;
    let join = thread::spawn(move || {
        let mut rpc = RpcService::new(ValidatorNode::new(VALIDATOR_ID, state));
        for _ in 0..request_count {
            server
                .serve_next_connection(&mut rpc)
                .map_err(format_transport_error)?;
        }
        Ok(())
    });
    Ok((addr, RpcServerHandle { join: Some(join) }))
}

pub fn spawn_tcp_persistent_node(
    mut node: PersistentValidatorNode,
) -> Result<(SocketAddr, RpcServerHandle), String> {
    let server = JsonRpcServer::bind("127.0.0.1:0").map_err(format_transport_error)?;
    let addr = server.local_addr().map_err(format_transport_error)?;
    let join = thread::spawn(move || {
        server
            .serve_next_connection_with_handler(&mut node)
            .map_err(format_transport_error)
    });
    Ok((addr, RpcServerHandle { join: Some(join) }))
}

pub fn spawn_snapshot_sync_peer(
    node: PersistentValidatorNode,
    max_chunk_bytes: usize,
) -> Result<(SocketAddr, RpcServerHandle), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    let addr = listener.local_addr().map_err(|error| error.to_string())?;
    let join = thread::spawn(move || {
        let (stream, _) = listener.accept().map_err(|error| error.to_string())?;
        let mut protocol = TcpProtocolStream::from_stream(stream);
        while let Ok(message) = protocol.receive() {
            match message {
                NetworkMessage::SnapshotChunkRequest(request) => {
                    for response in node
                        .serve_snapshot_chunk_request(&request, max_chunk_bytes)
                        .map_err(|error| format!("{error:?}"))?
                    {
                        protocol
                            .send(&response)
                            .map_err(|error| format!("{error:?}"))?;
                    }
                }
                message => {
                    return Err(format!(
                        "expected snapshot chunk request, got {:?}",
                        message.kind()
                    ));
                }
            }
        }
        Ok(())
    });
    Ok((addr, RpcServerHandle { join: Some(join) }))
}

fn format_transport_error(error: RpcTransportError) -> String {
    format!("{error:?}")
}
