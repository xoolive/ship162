use std::{
    borrow::Cow,
    fmt,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct RetryDelay(Duration);

impl RetryDelay {
    const DEFAULT: Self = Self(Duration::from_secs(5));

    pub(crate) const fn duration(self) -> Duration {
        self.0
    }
}

impl TryFrom<f64> for RetryDelay {
    type Error = &'static str;

    fn try_from(seconds: f64) -> Result<Self, Self::Error> {
        if !seconds.is_finite() || seconds <= 0.0 {
            return Err("retry delay must be finite and greater than zero");
        }

        let duration =
            Duration::try_from_secs_f64(seconds).map_err(|_| "retry delay is out of range")?;
        if duration.is_zero() {
            return Err("retry delay is too small");
        }

        Ok(Self(duration))
    }
}

impl From<RetryDelay> for f64 {
    fn from(delay: RetryDelay) -> Self {
        delay.duration().as_secs_f64()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum RetryPolicy {
    Fixed {
        #[serde(rename = "delay_seconds")]
        delay: RetryDelay,
    },
    // TODO we might want to implement exponential backoff but keeping it simple for now
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::Fixed {
            delay: RetryDelay::DEFAULT,
        }
    }
}

impl RetryPolicy {
    fn delay(self, _attempt: u32) -> Duration {
        match self {
            Self::Fixed { delay } => delay.duration(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(usize);

impl SourceId {
    pub(crate) const fn from_index(index: usize) -> Self {
        Self(index + 1)
    }

    const fn get(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceStatus {
    Connecting,
    Connected,
    Disconnected {
        reason: Cow<'static, str>,
        retry_in: Option<Duration>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEvent {
    source_id: SourceId,
    source: Arc<str>,
    status: SourceStatus,
}

impl SourceEvent {
    pub(crate) const fn source_id(&self) -> SourceId {
        self.source_id
    }

    pub(crate) fn log(&self) {
        match &self.status {
            SourceStatus::Connecting => {
                tracing::info!(
                    source_id = self.source_id.get(),
                    source = self.source.as_ref(),
                    status = "connecting",
                    "source connecting"
                );
            }
            SourceStatus::Connected => {
                tracing::info!(
                    source_id = self.source_id.get(),
                    source = self.source.as_ref(),
                    status = "connected",
                    "source connected"
                );
            }
            SourceStatus::Disconnected {
                reason,
                retry_in: Some(retry_in),
            } => {
                tracing::warn!(
                    source_id = self.source_id.get(),
                    source = self.source.as_ref(),
                    status = "disconnected",
                    reason = reason.as_ref(),
                    retry_seconds = retry_in.as_secs_f64(),
                    "source disconnected; retrying"
                );
            }
            SourceStatus::Disconnected {
                reason,
                retry_in: None,
            } => {
                tracing::warn!(
                    source_id = self.source_id.get(),
                    source = self.source.as_ref(),
                    status = "disconnected",
                    reason = reason.as_ref(),
                    "source disconnected"
                );
            }
        }
    }
}

impl fmt::Display for SourceEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.status {
            SourceStatus::Connecting => write!(f, "{}: connecting", self.source),
            SourceStatus::Connected => write!(f, "{}: connected", self.source),
            SourceStatus::Disconnected {
                reason,
                retry_in: Some(retry_in),
            } => write!(
                f,
                "{}: {reason}; retrying in {}s",
                self.source,
                retry_in.as_secs_f64()
            ),
            SourceStatus::Disconnected {
                reason,
                retry_in: None,
            } => write!(f, "{}: {reason}; disconnected", self.source),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceReporter {
    source_id: SourceId,
    source: Arc<str>,
    status_tx: UnboundedSender<SourceEvent>,
}

impl SourceReporter {
    pub(crate) fn new(
        source_id: SourceId,
        source: impl Into<Arc<str>>,
        status_tx: UnboundedSender<SourceEvent>,
    ) -> Self {
        Self {
            source_id,
            source: source.into(),
            status_tx,
        }
    }

    pub(crate) fn report(&self, status: SourceStatus) {
        let _ = self.status_tx.send(SourceEvent {
            source_id: self.source_id,
            source: Arc::clone(&self.source),
            status,
        });
    }
}

#[derive(Debug)]
pub struct SourceRuntime {
    reporter: SourceReporter,
    retry_policy: RetryPolicy,
    failed_attempts: AtomicU32,
}

impl SourceRuntime {
    pub(crate) fn new(
        source_id: SourceId,
        source: impl Into<Arc<str>>,
        status_tx: UnboundedSender<SourceEvent>,
        retry_policy: RetryPolicy,
    ) -> Self {
        Self {
            reporter: SourceReporter::new(source_id, source, status_tx),
            retry_policy,
            failed_attempts: AtomicU32::new(0),
        }
    }

    pub(crate) fn report(&self, status: SourceStatus) {
        if matches!(&status, SourceStatus::Connected) {
            self.failed_attempts.store(0, Ordering::Relaxed);
        }
        self.reporter.report(status);
    }

    fn next_retry_in(&self) -> Duration {
        let attempt = self.failed_attempts.fetch_add(1, Ordering::Relaxed);
        self.retry_policy.delay(attempt)
    }
}

pub(crate) trait ReconnectingSource {
    fn runtime(&self) -> &SourceRuntime;

    async fn connect_once(&self) -> anyhow::Result<()>;

    async fn run(&self) {
        loop {
            self.runtime().report(SourceStatus::Connecting);
            let reason = match self.connect_once().await {
                Ok(()) => Cow::Borrowed("connection closed"),
                Err(error) => Cow::Owned(error.to_string()),
            };
            let retry_in = self.runtime().next_retry_in();
            self.runtime().report(SourceStatus::Disconnected {
                reason,
                retry_in: Some(retry_in),
            });
            tokio::time::sleep(retry_in).await;
        }
    }
}
