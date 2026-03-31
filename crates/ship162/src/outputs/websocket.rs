use futures::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{info, warn};

pub async fn run_websocket_server(
    addr: SocketAddr,
    mut shutdown_rx: broadcast::Receiver<()>,
    nmea_tx: broadcast::Sender<String>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!("WebSocket output server listening on ws://{}", addr);

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, peer_addr)) => {
                        info!("WebSocket client connecting: {}", peer_addr);
                        let mut client_rx = nmea_tx.subscribe();
                        tokio::spawn(async move {
                            match accept_async(stream).await {
                                Ok(ws_stream) => {
                                    let (mut write, mut read) = ws_stream.split();
                                    // Drain incoming messages (pings, close frames)
                                    let drain = tokio::spawn(async move {
                                        while let Some(Ok(_)) = read.next().await {}
                                    });

                                    loop {
                                        match client_rx.recv().await {
                                            Ok(line) => {
                                                if write.send(WsMessage::Text(line.into())).await.is_err() {
                                                    break;
                                                }
                                            }
                                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                                warn!("WebSocket client {} lagged by {} messages", peer_addr, n);
                                            }
                                            Err(broadcast::error::RecvError::Closed) => break,
                                        }
                                    }
                                    drain.abort();
                                    info!("WebSocket client disconnected: {}", peer_addr);
                                }
                                Err(e) => {
                                    warn!("WebSocket handshake failed for {}: {}", peer_addr, e);
                                }
                            }
                        });
                    }
                    Err(e) => warn!("WebSocket accept error: {}", e),
                }
            }
            _ = shutdown_rx.recv() => {
                info!("WebSocket output server shutting down");
                break;
            }
        }
    }

    Ok(())
}
