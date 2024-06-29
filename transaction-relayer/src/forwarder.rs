use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{Builder, JoinHandle},
    time::{Duration, Instant, SystemTime},
};

use crate::scorer::TrafficScorer;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use jito_block_engine::block_engine::BlockEnginePackets;
use jito_relayer::relayer::RelayerPacketBatches;
use solana_core::banking_trace::BankingPacketBatch;
use solana_metrics::datapoint_info;
use tokio::sync::mpsc::error::TrySendError;

pub const BLOCK_ENGINE_FORWARDER_QUEUE_CAPACITY: usize = 5_000;

/// Forwards packets to the Block Engine handler thread.
/// Delays transactions for packet_delay_ms before forwarding them to the validator.
pub fn start_traffic_scorer_threads(
    verified_receiver: Receiver<BankingPacketBatch>,
    outgoing_packet_sender: Sender<RelayerPacketBatches>,
    packet_delay_ms: u32,
    num_threads: u64,
    scorer: Arc<TrafficScorer>,
    exit: &Arc<AtomicBool>,
) -> Vec<JoinHandle<()>> {
    const SLEEP_DURATION: Duration = Duration::from_millis(5);

    (0..num_threads)
        .map(|thread_id| {
            let verified_receiver = verified_receiver.clone();
            let delay_packet_sender = outgoing_packet_sender.clone();

            let exit = exit.clone();
            let co_scorer = scorer.clone();
            Builder::new()
                .name(format!("forwarder_thread_{thread_id}"))
                .spawn(move || {
                    let mut buffered_packet_batches: VecDeque<RelayerPacketBatches> =
                        VecDeque::with_capacity(100_000);

                    let metrics_interval = Duration::from_secs(1);
                    let mut forwarder_metrics = ForwarderMetrics::new(
                        buffered_packet_batches.capacity(),
                        verified_receiver.capacity().unwrap_or_default(), // TODO (LB): unbounded channel now, remove metric
                        0,
                    );
                    let mut last_metrics_upload = Instant::now();

                    while !exit.load(Ordering::Relaxed) {
                        if last_metrics_upload.elapsed() >= metrics_interval {
                            forwarder_metrics.report(thread_id, packet_delay_ms);

                            forwarder_metrics = ForwarderMetrics::new(
                                buffered_packet_batches.capacity(),
                                verified_receiver.capacity().unwrap_or_default(), // TODO (LB): unbounded channel now, remove metric
                                0,
                            );
                            last_metrics_upload = Instant::now();
                        }

                        match verified_receiver.recv_timeout(SLEEP_DURATION) {
                            Ok(banking_packet_batch) => {
                                let instant = Instant::now();
                                let num_packets = banking_packet_batch
                                    .0
                                    .iter()
                                    .map(|b| b.len() as u64)
                                    .sum::<u64>();
                                forwarder_metrics.num_batches_received += 1;
                                forwarder_metrics.num_packets_received += num_packets;
                                let stats = co_scorer.score(&banking_packet_batch);

                                buffered_packet_batches.push_back(RelayerPacketBatches {
                                    stamp: instant,
                                    banking_packet_batch,
                                });
                            }
                            Err(RecvTimeoutError::Timeout) => {}
                            Err(RecvTimeoutError::Disconnected) => {
                                panic!("packet receiver disconnected");
                            }
                        }

                        while let Some(packet_batches) = buffered_packet_batches.pop_front() {
                            let num_packets = packet_batches
                                .banking_packet_batch
                                .0
                                .iter()
                                .map(|b| b.len() as u64)
                                .sum::<u64>();

                            forwarder_metrics.num_relayer_packets_forwarded += num_packets;
                            delay_packet_sender
                                .send(packet_batches)
                                .expect("exiting forwarding delayed packets");
                        }

                        forwarder_metrics.update_queue_lengths(
                            buffered_packet_batches.len(),
                            buffered_packet_batches.capacity(),
                            verified_receiver.len(),
                            BLOCK_ENGINE_FORWARDER_QUEUE_CAPACITY - 0,
                        );
                    }
                })
                .unwrap()
        })
        .collect()
}

struct ForwarderMetrics {
    pub num_batches_received: u64,
    pub num_packets_received: u64,

    pub num_be_packets_forwarded: u64,
    pub num_be_packets_dropped: u64,
    pub num_be_sender_full: u64,

    pub num_relayer_packets_forwarded: u64,

    pub buffered_packet_batches_max_len: usize,
    pub buffered_packet_batches_capacity: usize,
    pub verified_receiver_max_len: usize,
    pub verified_receiver_capacity: usize,
    pub block_engine_sender_max_len: usize,
    pub block_engine_sender_capacity: usize,
}

impl ForwarderMetrics {
    pub fn new(
        buffered_packet_batches_capacity: usize,
        verified_receiver_capacity: usize,
        block_engine_sender_capacity: usize,
    ) -> Self {
        ForwarderMetrics {
            num_batches_received: 0,
            num_packets_received: 0,
            num_be_packets_forwarded: 0,
            num_be_packets_dropped: 0,
            num_be_sender_full: 0,
            num_relayer_packets_forwarded: 0,
            buffered_packet_batches_max_len: 0,
            buffered_packet_batches_capacity,
            verified_receiver_max_len: 0,
            verified_receiver_capacity,
            block_engine_sender_max_len: 0,
            block_engine_sender_capacity,
        }
    }

    pub fn update_queue_lengths(
        &mut self,
        buffered_packet_batches_len: usize,
        buffered_packet_batches_capacity: usize,
        verified_receiver_len: usize,
        block_engine_sender_len: usize,
    ) {
        self.buffered_packet_batches_max_len = std::cmp::max(
            self.buffered_packet_batches_max_len,
            buffered_packet_batches_len,
        );
        self.buffered_packet_batches_capacity = std::cmp::max(
            self.buffered_packet_batches_capacity,
            buffered_packet_batches_capacity,
        );
        self.verified_receiver_max_len =
            std::cmp::max(self.verified_receiver_max_len, verified_receiver_len);

        self.block_engine_sender_max_len =
            std::cmp::max(self.block_engine_sender_max_len, block_engine_sender_len);
    }

    pub fn report(&self, thread_id: u64, delay: u32) {
        datapoint_info!(
            "forwarder_metrics",
            ("thread_id", thread_id, i64),
            ("delay", delay, i64),
            ("num_batches_received", self.num_batches_received, i64),
            ("num_packets_received", self.num_packets_received, i64),
            // Relayer -> Block Engine Metrics
            (
                "num_be_packets_forwarded",
                self.num_be_packets_forwarded,
                i64
            ),
            ("num_be_packets_dropped", self.num_be_packets_dropped, i64),
            ("num_be_sender_full", self.num_be_sender_full, i64),
            // Relayer -> validator metrics
            (
                "num_relayer_packets_forwarded",
                self.num_relayer_packets_forwarded,
                i64
            ),
            // Channel stats
            (
                "buffered_packet_batches_len",
                self.buffered_packet_batches_max_len,
                i64
            ),
            (
                "buffered_packet_batches_capacity",
                self.buffered_packet_batches_capacity,
                i64
            ),
            ("verified_receiver_len", self.verified_receiver_max_len, i64),
            (
                "verified_receiver_capacity",
                self.verified_receiver_capacity,
                i64
            ),
            (
                "block_engine_sender_len",
                self.block_engine_sender_max_len,
                i64
            ),
            (
                "block_engine_sender_capacity",
                self.block_engine_sender_capacity,
                i64
            ),
        );
    }
}
