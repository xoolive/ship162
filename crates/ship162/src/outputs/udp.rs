use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

/// Run a UDP output server that:
/// - Sends NMEA to all static targets (configured addresses)
/// - Accepts dynamic peer registration (clients send any datagram to register)
pub async fn run_udp_server(
    addr: SocketAddr,
    static_targets: Vec<SocketAddr>,
    mut shutdown_rx: broadcast::Receiver<()>,
    mut nmea_rx: broadcast::Receiver<String>,
) -> anyhow::Result<()> {
    let socket = Arc::new(UdpSocket::bind(addr).await?);
    info!("UDP output server listening on {}", addr);

    let mut peers: HashSet<SocketAddr> = HashSet::new();
    for target in &static_targets {
        info!("UDP static target: {}", target);
        peers.insert(*target);
    }
    let peers = Arc::new(RwLock::new(peers));

    // Accept dynamic peer registrations
    let socket_recv = socket.clone();
    let peers_recv = peers.clone();
    let recv_handle = tokio::spawn(async move {
        let mut buf = [0u8; 64];
        loop {
            match socket_recv.recv_from(&mut buf).await {
                Ok((_, peer_addr)) => {
                    let mut set = peers_recv.write().await;
                    if set.insert(peer_addr) {
                        info!("UDP peer registered: {}", peer_addr);
                    }
                }
                Err(e) => {
                    warn!("UDP recv error: {}", e);
                    break;
                }
            }
        }
    });

    loop {
        tokio::select! {
            result = nmea_rx.recv() => {
                match result {
                    Ok(line) => {
                        let set = peers.read().await;
                        for peer in set.iter() {
                            let _ = socket.send_to(line.as_bytes(), peer).await;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("UDP output lagged by {} messages", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = shutdown_rx.recv() => {
                info!("UDP output server shutting down");
                break;
            }
        }
    }

    recv_handle.abort();
    Ok(())
}
