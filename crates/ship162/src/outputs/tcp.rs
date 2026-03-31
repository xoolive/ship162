use std::net::SocketAddr;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing::{info, warn};

pub async fn run_tcp_server(
    addr: SocketAddr,
    mut shutdown_rx: broadcast::Receiver<()>,
    nmea_tx: broadcast::Sender<String>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!("TCP output server listening on {}", addr);

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((mut socket, peer_addr)) => {
                        info!("TCP client connected: {}", peer_addr);
                        let mut client_rx = nmea_tx.subscribe();
                        tokio::spawn(async move {
                            loop {
                                match client_rx.recv().await {
                                    Ok(line) => {
                                        if socket.write_all(line.as_bytes()).await.is_err() {
                                            info!("TCP client disconnected: {}", peer_addr);
                                            break;
                                        }
                                    }
                                    Err(broadcast::error::RecvError::Lagged(n)) => {
                                        warn!("TCP client {} lagged by {} messages", peer_addr, n);
                                    }
                                    Err(broadcast::error::RecvError::Closed) => break,
                                }
                            }
                        });
                    }
                    Err(e) => warn!("TCP accept error: {}", e),
                }
            }
            _ = shutdown_rx.recv() => {
                info!("TCP output server shutting down");
                break;
            }
        }
    }

    Ok(())
}
