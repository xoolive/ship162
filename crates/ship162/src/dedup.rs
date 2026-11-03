//! Merge receptions of the same AIS message heard through several sources.
//!
//! One buffer, two stages:
//!
//! - **exact**: two receptions that both carry raw AIS bits (NMEA over TCP or
//!   WebSocket, IQ receivers) are the same transmission when the bits are
//!   identical and their timestamps are less than `exact` milliseconds apart,
//!   as in jet1090;
//! - **soft**: when either one comes from a decoded feed (Digitraffic MQTT,
//!   aisstream.io), there are no bits to compare. Two receptions are then the
//!   same report when they have the same MMSI and position (at the AIS
//!   resolution of 1/10000 minute) for position reports, or the same decoded
//!   content otherwise, and their timestamps are less than `soft`
//!   milliseconds apart. Aggregated feeds relay the same reports: for vessels
//!   in both Digitraffic and aisstream.io, almost all positions are identical
//!   and at most a second or so apart.
//!
//! Receptions from the same source are never merged, and a reception
//! compatible with more than one pending group is kept separate rather than
//! paired arbitrarily. A merged message keeps the earliest timestamp and every
//! reception in its `metadata`, so no provenance is lost. Messages are held for
//! the longer of both windows (wall clock) before they are passed on.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use rs162::prelude::Message;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::Instant;

use crate::sources::TimedMessage;

/// Default window for identical raw AIS bits, in milliseconds
pub const DEFAULT_EXACT_MS: u32 = 1000;
/// Default window for decoded feeds, in milliseconds
pub const DEFAULT_SOFT_MS: u32 = 3000;

const MAX_PENDING_GROUPS: usize = 4096;
const MAX_GROUP_RECEPTIONS: usize = 64;

/// Deduplication windows, in milliseconds
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub exact_ms: u32,
    pub soft_ms: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            exact_ms: DEFAULT_EXACT_MS,
            soft_ms: DEFAULT_SOFT_MS,
        }
    }
}

impl Settings {
    /// How long a group stays open for further receptions
    fn collect(&self) -> Duration {
        Duration::from_millis(u64::from(self.exact_ms.max(self.soft_ms)))
    }
}

/// What two receptions must share to be compared at all
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    /// Position reports: the same vessel at the same place
    Position {
        mmsi: u32,
        latitude: i64,
        longitude: i64,
    },
    /// Anything else: the same decoded content
    Content(String),
}

/// Positions at the AIS resolution (1/10000 minute) absorb float rounding
/// in feeds that publish decimal degrees.
fn quantize(degrees: f64) -> i64 {
    (degrees * 600_000.).round() as i64
}

fn key(message: &Message) -> Key {
    let position = match message {
        Message::PositionReport1(msg)
        | Message::PositionReport2(msg)
        | Message::PositionReport3(msg) => Some((msg.mmsi, msg.latitude, msg.longitude)),
        Message::ClassBPositionReport(msg) => Some((msg.mmsi, msg.latitude, msg.longitude)),
        Message::ExtendedClassBPositionReport(msg) => Some((msg.mmsi, msg.latitude, msg.longitude)),
        _ => None,
    };
    match position {
        Some((mmsi, Some(latitude), Some(longitude))) => Key::Position {
            mmsi,
            latitude: quantize(latitude),
            longitude: quantize(longitude),
        },
        _ => {
            Key::Content(serde_json::to_string(message).unwrap_or_else(|_| format!("{message:?}")))
        }
    }
}

type GroupId = (Instant, u64);

struct Group {
    key: Key,
    entries: Vec<TimedMessage>,
    earliest: f64,
    latest: f64,
}

impl Group {
    fn compatible(&self, msg: &TimedMessage, settings: Settings) -> bool {
        if self.entries.len() >= MAX_GROUP_RECEPTIONS || !msg.timestamp.is_finite() {
            return false;
        }
        // Identical bits are required whenever both sides have them
        let exact = msg.payload.is_some() && self.entries.iter().all(|e| e.payload.is_some());
        if let Some(payload) = &msg.payload {
            let differs = self
                .entries
                .iter()
                .any(|e| e.payload.as_ref().is_some_and(|other| other != payload));
            if differs {
                return false;
            }
        }
        let threshold = if exact {
            settings.exact_ms
        } else {
            settings.soft_ms
        };
        let span = self.latest.max(msg.timestamp) - self.earliest.min(msg.timestamp);
        if !(span.is_finite() && span * 1000. < f64::from(threshold)) {
            return false;
        }
        // Receptions from one source are distinct transmissions (or a feed
        // repeating itself), never merged.
        let sources: Vec<_> = msg.metadata.iter().map(|r| r.source_id).collect();
        !sources.is_empty()
            && sources.iter().all(Option::is_some)
            && !self
                .entries
                .iter()
                .flat_map(|e| &e.metadata)
                .any(|r| r.source_id.is_none() || sources.contains(&r.source_id))
    }

    /// One message for the whole group: the raw reception if any (it has the
    /// bits and NMEA sentences), with the earliest timestamp and all receptions.
    fn merge(mut self) -> TimedMessage {
        let index = self
            .entries
            .iter()
            .position(|e| e.payload.is_some())
            .unwrap_or(0);
        let mut merged = self.entries.swap_remove(index);
        merged.timestamp = self.earliest;
        for entry in self.entries {
            merged.metadata.extend(entry.metadata);
        }
        merged
            .metadata
            .sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
        merged
    }
}

/// Groups waiting for more receptions, by deadline
#[derive(Default)]
struct Pending {
    groups: BTreeMap<GroupId, Group>,
    by_key: HashMap<Key, Vec<GroupId>>,
    next_id: u64,
}

impl Pending {
    fn insert(&mut self, msg: TimedMessage, now: Instant, settings: Settings) {
        let key = key(&msg.message);
        let candidates: Vec<GroupId> = self
            .by_key
            .get(&key)
            .into_iter()
            .flatten()
            .filter(|id| self.groups[*id].compatible(&msg, settings))
            .copied()
            .take(2)
            .collect();
        if let [id] = candidates[..] {
            let group = self.groups.get_mut(&id).expect("indexed group");
            group.earliest = group.earliest.min(msg.timestamp);
            group.latest = group.latest.max(msg.timestamp);
            group.entries.push(msg);
            return;
        }
        let id = (now + settings.collect(), self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.by_key.entry(key.clone()).or_default().push(id);
        self.groups.insert(
            id,
            Group {
                key,
                earliest: msg.timestamp,
                latest: msg.timestamp,
                entries: vec![msg],
            },
        );
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.groups.first_key_value().map(|(id, _)| id.0)
    }

    /// The oldest group, if its deadline has passed, the buffer is full, or
    /// `flush` is set (end of input).
    fn pop(&mut self, now: Instant, flush: bool) -> Option<TimedMessage> {
        let (id, _) = self.groups.first_key_value()?;
        if !flush && id.0 > now && self.groups.len() < MAX_PENDING_GROUPS {
            return None;
        }
        let (id, group) = self.groups.pop_first()?;
        if let Some(ids) = self.by_key.get_mut(&group.key) {
            ids.retain(|other| *other != id);
            if ids.is_empty() {
                self.by_key.remove(&group.key);
            }
        }
        Some(group.merge())
    }
}

/// Forward messages from `rx` to `tx`, merging duplicates.
/// Returns when `rx` is closed (after flushing) or `tx` is dropped.
pub async fn deduplicate(
    mut rx: Receiver<TimedMessage>,
    tx: Sender<TimedMessage>,
    settings: Settings,
) {
    let mut pending = Pending::default();
    let mut eof = false;
    loop {
        while let Some(msg) = pending.pop(Instant::now(), eof) {
            if tx.send(msg).await.is_err() {
                return;
            }
        }
        if eof {
            return;
        }
        let deadline = pending.next_deadline();
        tokio::select! {
            biased;
            msg = rx.recv() => match msg {
                Some(msg) => pending.insert(msg, Instant::now(), settings),
                None => eof = true,
            },
            _ = async {
                match deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending().await,
                }
            } => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::Reception;
    use crate::status::SourceId;
    use rs162::decode::ais::common::Timestamp;
    use rs162::prelude::{ManeuverIndicator, NavigationStatus, PositionReport};

    fn position(mmsi: u32, latitude: f64, longitude: f64) -> Message {
        Message::PositionReport1(PositionReport {
            msg_type: 1,
            repeat: 0,
            mmsi,
            status: NavigationStatus::from_bits(0),
            turn: None,
            speed: Some(12.3),
            accuracy: false,
            longitude: Some(longitude),
            latitude: Some(latitude),
            course: Some(90.),
            heading: Some(90),
            second: Timestamp::NotAvailable,
            maneuver: ManeuverIndicator::NotAvailable,
            spare_1: 0,
            raim: false,
            radio: 0,
        })
    }

    fn reception(
        source: usize,
        timestamp: f64,
        message: Message,
        payload: Option<&[u8]>,
    ) -> TimedMessage {
        TimedMessage {
            timestamp,
            signal_level: None,
            message,
            mmsi_info: None,
            nmea_sentences: vec![],
            payload: payload.map(<[u8]>::to_vec),
            metadata: vec![Reception {
                source: format!("source {source}"),
                timestamp,
                signal_level: None,
                source_id: Some(SourceId::from_index(source)),
            }],
        }
    }

    fn run(messages: Vec<TimedMessage>) -> Vec<TimedMessage> {
        let now = Instant::now();
        let mut pending = Pending::default();
        for msg in messages {
            pending.insert(msg, now, Settings::default());
        }
        std::iter::from_fn(|| pending.pop(now, true)).collect()
    }

    #[test]
    fn identical_bits_from_two_sources_merge() {
        let msg = position(244_123_456, 52.0, 4.0);
        let out = run(vec![
            reception(0, 100.0, msg.clone(), Some(b"bits")),
            reception(1, 100.4, msg, Some(b"bits")),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].timestamp, 100.0);
        assert_eq!(out[0].metadata.len(), 2);
        assert_eq!(out[0].metadata[1].source, "source 1");
    }

    #[test]
    fn different_bits_never_merge() {
        // Same position, different radio state: two transmissions
        let msg = position(244_123_456, 52.0, 4.0);
        let out = run(vec![
            reception(0, 100.0, msg.clone(), Some(b"bits")),
            reception(1, 100.1, msg, Some(b"other")),
        ]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn exact_window_is_shorter_than_soft() {
        let msg = position(244_123_456, 52.0, 4.0);
        let out = run(vec![
            reception(0, 100.0, msg.clone(), Some(b"bits")),
            reception(1, 101.5, msg, Some(b"bits")),
        ]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn decoded_feeds_merge_on_position() {
        // Digitraffic rounds degrees to 6 decimals, aisstream.io does not
        let out = run(vec![
            reception(0, 100.0, position(230_000_001, 60.150685, 24.9), None),
            reception(
                1,
                101.2,
                position(230_000_001, 60.1506853, 24.9000002),
                None,
            ),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].metadata.len(), 2);
    }

    #[test]
    fn raw_and_decoded_merge_and_keep_the_bits() {
        let msg = position(230_000_001, 60.0, 25.0);
        let out = run(vec![
            reception(0, 100.0, msg.clone(), None),
            reception(1, 100.5, msg, Some(b"bits")),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].payload.as_deref(), Some(&b"bits"[..]));
        assert_eq!(out[0].timestamp, 100.0);
    }

    #[test]
    fn same_source_never_merges() {
        // A moored ship repeating its position is not a duplicate
        let msg = position(230_000_001, 60.0, 25.0);
        let out = run(vec![
            reception(0, 100.0, msg.clone(), None),
            reception(0, 101.0, msg, None),
        ]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn soft_window_and_other_vessels() {
        let out = run(vec![
            reception(0, 100.0, position(230_000_001, 60.0, 25.0), None),
            reception(1, 103.5, position(230_000_001, 60.0, 25.0), None),
            reception(1, 100.0, position(230_000_002, 60.0, 25.0), None),
            reception(1, 100.0, position(230_000_001, 60.001, 25.0), None),
        ]);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn ambiguous_receptions_stay_separate() {
        // Two pending groups from source 0 (same report twice, 1 s apart),
        // then source 1: it could pair with either, so it pairs with none.
        let msg = position(230_000_001, 60.0, 25.0);
        let out = run(vec![
            reception(0, 100.0, msg.clone(), None),
            reception(0, 101.0, msg.clone(), None),
            reception(1, 100.5, msg, None),
        ]);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn groups_wait_for_their_deadline() {
        let now = Instant::now();
        let mut pending = Pending::default();
        let msg = position(230_000_001, 60.0, 25.0);
        pending.insert(reception(0, 100.0, msg, None), now, Settings::default());
        assert!(pending.pop(now, false).is_none());
        let later = now + Settings::default().collect();
        assert!(pending.pop(later, false).is_some());
        assert!(pending.by_key.is_empty());
    }

    #[tokio::test]
    async fn task_flushes_at_end_of_input() {
        let (in_tx, in_rx) = tokio::sync::mpsc::channel(8);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(8);
        let task = tokio::spawn(deduplicate(in_rx, out_tx, Settings::default()));
        let msg = position(230_000_001, 60.0, 25.0);
        in_tx
            .send(reception(0, 100.0, msg.clone(), None))
            .await
            .unwrap();
        in_tx.send(reception(1, 100.2, msg, None)).await.unwrap();
        drop(in_tx);
        let merged = out_rx.recv().await.expect("one message");
        assert_eq!(merged.metadata.len(), 2);
        assert!(out_rx.recv().await.is_none());
        task.await.unwrap();
    }
}
