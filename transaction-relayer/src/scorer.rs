use crate::db_service::TransactionRow;
use jito_core::immutable_deserialized_packet::ImmutableDeserializedPacket;
use log::{error, info};
use solana_core::banking_trace::BankingPacketBatch;
use solana_program::instruction::InstructionError::ProgramFailedToCompile;
use solana_program::message::Message;
use solana_program::sanitize::SanitizeError;
use solana_program::short_vec::decode_shortu16_len;
use solana_runtime::transaction_priority_details::GetTransactionPriorityDetails;
use solana_sdk::packet::{Packet, PacketFlags};
use solana_sdk::signature::Signature;
use solana_sdk::transaction::{SanitizedVersionedTransaction, VersionedTransaction};
use std::mem::size_of;
use std::net::{SocketAddr, SocketAddrV4};
use solana_perf::packet::PacketBatch;
use thiserror::Error;
use tokio::sync::mpsc::{Sender, UnboundedSender};

pub struct TrafficScorer {
    db_channel: UnboundedSender<TransactionRow>,
}

#[derive(Debug)]
pub struct ScoringStats {
    pub total_packets: u64,
    pub failed_decoding: u64,
    pub failed_priority: u64,
}

impl Default for ScoringStats {
    fn default() -> Self {
        ScoringStats {
            total_packets: 0,
            failed_decoding: 0,
            failed_priority: 0,
        }
    }
}


fn generate_packet_indexes(packet_batch: &PacketBatch) -> Vec<usize> {
    packet_batch
        .iter()
        .enumerate()
        .filter(|(_, pkt)| !pkt.meta().discard())
        .map(|(index, _)| index)
        .collect()
}

impl TrafficScorer {
    pub fn new(db_channel: UnboundedSender<TransactionRow>) -> Self {
        TrafficScorer { db_channel }
    }

    pub fn score(&self, traffic: &BankingPacketBatch) -> ScoringStats {
        let unix_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut stats = ScoringStats::default();
        for batch in traffic.0.iter() {
            let packet_indices = generate_packet_indexes(batch);
            let packet_iter = packet_indices.iter().filter_map(move |packet_index| {
                let mut packet_clone = batch[*packet_index].clone();
                packet_clone
                    .meta_mut()
                    .set_round_compute_unit_price(false);
                ImmutableDeserializedPacket::new(packet_clone)
                    .ok()
                    .filter(|_| true)
            });

            for packet in packet_iter {
                self.db_channel.send((unix_ts, packet).into()).unwrap();
            }
            // for idx in 0..batch.len() {
            //
            //
            //     let raw_packet = batch[idx].clone();
            //     stats.total_packets += 1;
            //     match ImmutableDeserializedPacket::new(raw_packet) {
            //         Ok(packet) => {
            //             self.db_channel.send((unix_ts, packet).into()).unwrap();
            //         }
            //         Err(e) => {
            //             error!("Error decoding packet: {e}");
            //             stats.failed_decoding += 1;
            //         }
            //     };
            // }
        }

        stats
    }
}
