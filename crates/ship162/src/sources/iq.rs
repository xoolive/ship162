use std::future::Future;

use futures::StreamExt;
use rs162::sources::iq::AisAsyncIqSource;
use tokio::sync::{broadcast, mpsc::Sender};
use tracing::warn;

use crate::{
    sources::TimedMessage,
    status::{SourceRuntime, SourceStatus},
};

pub struct Source {
    tx: Sender<TimedMessage>,
    source: AisAsyncIqSource,
}

impl Source {
    pub fn new(tx: Sender<TimedMessage>, source: AisAsyncIqSource) -> Self {
        Self { tx, source }
    }

    /// Wait for the owned IQ reader to finish hardware cleanup.
    pub async fn stop(&mut self) -> anyhow::Result<()> {
        self.source.stop().await?;
        Ok(())
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        while let Some(msg_res) = self.source.next().await {
            let msg = msg_res?;
            let encoded = msg.encode_nmea();
            if let Some(ais_msg) = encoded.decode() {
                let sentence = super::TimedMessage {
                    timestamp: encoded.timestamp,
                    signal_level: Some(encoded.signal_level),
                    message: ais_msg,
                    mmsi_info: None,
                    nmea_sentences: encoded.nmea_sentences,
                };
                self.tx.send(sentence).await?;
            }
        }
        Ok(())
    }
}

/// Own an IQ reader until cleanup completes, including cancellation and failures.
/// Finite recordings finish once; live receivers reopen using the configured retry policy.
/// The factory also lets tests exercise failures without opening USB hardware.
pub async fn supervise<F, Fut>(
    tx: Sender<TimedMessage>,
    runtime: SourceRuntime,
    mut shutdown: broadcast::Receiver<()>,
    live: bool,
    mut open: F,
) where
    F: FnMut() -> Fut,
    Fut: Future<Output = desperado::Result<AisAsyncIqSource>>,
{
    loop {
        runtime.report(SourceStatus::Connecting);
        if tx.is_closed() {
            break;
        }
        let opened = tokio::select! {
            biased;
            _ = shutdown.recv() => break,
            _ = tx.closed() => break,
            result = open() => result,
        };
        let result = match opened {
            Ok(source) => {
                runtime.report(SourceStatus::Connected);
                // Keep ownership outside select!: dropping run() must not skip stop/join.
                let mut source = Source::new(tx.clone(), source);
                let outcome = tokio::select! {
                    biased;
                    _ = shutdown.recv() => None,
                    _ = tx.closed() => None,
                    result = source.run() => Some(result),
                };
                let cleanup = source.stop().await;
                let Some(outcome) = outcome else {
                    if let Err(error) = cleanup {
                        warn!(%error, "Unable to stop IQ source cleanly");
                    }
                    break;
                };
                outcome.and(cleanup)
            }
            Err(error) => Err(error.into()),
        };
        if tx.is_closed() {
            break;
        }
        let reason = match result {
            Ok(()) if !live => {
                runtime.report(SourceStatus::Finished);
                return;
            }
            Ok(()) => "IQ stream ended unexpectedly".to_string(),
            Err(error) => format!("{error:#}"),
        };
        let retry_in = live.then(|| runtime.next_retry_in());
        runtime.report(SourceStatus::Disconnected {
            reason: reason.into(),
            retry_in,
        });
        let Some(delay) = retry_in else { return };
        tokio::select! {
            biased;
            _ = shutdown.recv() => break,
            _ = tx.closed() => break,
            _ = tokio::time::sleep(delay) => {},
        }
    }
    runtime.report(SourceStatus::Stopped);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::{RetryPolicy, SourceEvent, SourceId};
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };
    use tokio::sync::mpsc;
    use tokio::time::timeout;

    fn runtime() -> (SourceRuntime, mpsc::UnboundedReceiver<SourceEvent>) {
        let (status_tx, status_rx) = mpsc::unbounded_channel();
        let policy = RetryPolicy::Fixed {
            delay: 0.001_f64.try_into().unwrap(),
        };
        (
            SourceRuntime::new(SourceId::from_index(0), "test-iq", status_tx, policy),
            status_rx,
        )
    }

    fn fake_source(error: Option<&str>) -> AisAsyncIqSource {
        let (tx, rx) = mpsc::channel(1);
        let error = error.map(str::to_owned);
        let handle = tokio::spawn(async move {
            if let Some(error) = error {
                let _ = tx.send(Err(desperado::Error::other(error))).await;
            }
        });
        AisAsyncIqSource::new(handle, rx)
    }

    #[tokio::test]
    async fn read_errors_reach_the_application() {
        let (tx, _rx) = mpsc::channel(1);
        let mut source = Source::new(tx, fake_source(Some("injected read failure")));
        assert!(source
            .run()
            .await
            .unwrap_err()
            .to_string()
            .contains("injected read failure"));
        source.stop().await.unwrap();
    }

    #[tokio::test]
    async fn finite_eof_finishes_and_closes_message_channel() {
        let (tx, mut rx) = mpsc::channel(1);
        let (runtime, mut statuses) = runtime();
        let (_shutdown_tx, shutdown_rx) = broadcast::channel(1);
        supervise(tx, runtime, shutdown_rx, false, || async {
            Ok(fake_source(None))
        })
        .await;
        assert!(rx.recv().await.is_none());
        assert!(matches!(
            statuses.recv().await.unwrap().status(),
            SourceStatus::Connecting
        ));
        assert!(matches!(
            statuses.recv().await.unwrap().status(),
            SourceStatus::Connected
        ));
        assert!(matches!(
            statuses.recv().await.unwrap().status(),
            SourceStatus::Finished
        ));
    }

    #[tokio::test]
    async fn finite_open_failure_is_not_retried() {
        let (tx, _rx) = mpsc::channel(1);
        let (runtime, mut statuses) = runtime();
        let (_shutdown_tx, shutdown_rx) = broadcast::channel(1);
        supervise(tx, runtime, shutdown_rx, false, || async {
            Err(desperado::Error::other("missing capture"))
        })
        .await;
        statuses.recv().await.unwrap();
        let failed = statuses.recv().await.unwrap();
        assert!(failed.terminal_error().unwrap().contains("missing capture"));
        assert!(statuses.recv().await.is_none());
    }

    #[tokio::test]
    async fn live_open_and_read_failures_retry_and_shutdown_interrupts_backoff() {
        let (tx, _rx) = mpsc::channel(1);
        let (runtime, mut statuses) = runtime();
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let attempts = Arc::new(AtomicUsize::new(0));
        let observed = attempts.clone();
        let task = tokio::spawn(supervise(tx, runtime, shutdown_rx, true, move || {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
            async move {
                if attempt == 0 {
                    Err(desperado::Error::other("unplugged"))
                } else {
                    Ok(fake_source(Some("USB read failed")))
                }
            }
        }));
        timeout(Duration::from_secs(2), async {
            while let Some(event) = statuses.recv().await {
                if event.to_string().contains("USB read failed") {
                    break;
                }
            }
        })
        .await
        .unwrap();
        shutdown_tx.send(()).unwrap();
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        assert!(observed.load(Ordering::SeqCst) >= 2);
    }

    #[tokio::test]
    async fn shutdown_interrupts_an_open_in_progress() {
        let (tx, _rx) = mpsc::channel(1);
        let (runtime, mut statuses) = runtime();
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let task = tokio::spawn(supervise(tx, runtime, shutdown_rx, true, || {
            std::future::pending::<desperado::Result<AisAsyncIqSource>>()
        }));
        assert!(matches!(
            statuses.recv().await.unwrap().status(),
            SourceStatus::Connecting
        ));
        shutdown_tx.send(()).unwrap();
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            statuses.recv().await.unwrap().status(),
            SourceStatus::Stopped
        ));
    }

    #[tokio::test]
    async fn a_closed_consumer_does_not_reconnect_forever() {
        let (tx, rx) = mpsc::channel(1);
        let (runtime, mut statuses) = runtime();
        let (_shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let attempts = Arc::new(AtomicUsize::new(0));
        let observed = attempts.clone();
        drop(rx);
        timeout(
            Duration::from_secs(2),
            supervise(tx, runtime, shutdown_rx, true, move || {
                attempts.fetch_add(1, Ordering::SeqCst);
                async { Err(desperado::Error::other("must not open")) }
            }),
        )
        .await
        .unwrap();
        assert_eq!(observed.load(Ordering::SeqCst), 0);
        statuses.recv().await.unwrap();
        assert!(matches!(
            statuses.recv().await.unwrap().status(),
            SourceStatus::Stopped
        ));
    }

    #[tokio::test]
    async fn shutdown_joins_an_idle_worker() {
        let (tx, _rx) = mpsc::channel(1);
        let (runtime, mut statuses) = runtime();
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let stopped = Arc::new(AtomicUsize::new(0));
        let observed = stopped.clone();
        let task = tokio::spawn(supervise(tx, runtime, shutdown_rx, true, move || {
            let stopped = stopped.clone();
            async move {
                let (tx, rx) = mpsc::channel(1);
                let handle = tokio::spawn(async move {
                    tx.closed().await;
                    stopped.fetch_add(1, Ordering::SeqCst);
                });
                Ok(AisAsyncIqSource::new(handle, rx))
            }
        }));
        while !matches!(
            statuses.recv().await.unwrap().status(),
            SourceStatus::Connected
        ) {}
        shutdown_tx.send(()).unwrap();
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(observed.load(Ordering::SeqCst), 1);
    }
}
