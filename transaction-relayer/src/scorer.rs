
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
use jito_core::immutable_deserialized_packet::ImmutableDeserializedPacket;
use thiserror::Error;

pub struct TrafficScorer {}

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

impl TrafficScorer {
    pub fn new() -> TrafficScorer {
        TrafficScorer {}
    }

    pub fn score(&self, traffic: &BankingPacketBatch) -> ScoringStats {
        let mut stats = ScoringStats::default();
        for batch in traffic.0.iter() {
            for idx in 0..batch.len() {
                let raw_packet = batch[idx].clone();
                stats.total_packets += 1;
                match ImmutableDeserializedPacket::new(raw_packet) {
                    Ok(packet) => {
                        match packet.transaction().get_transaction_priority_details(false) {
                            Some(cu_details) => {
                                let prio_per_unit =
                                    cu_details.priority / cu_details.compute_unit_limit;
                                let hash = packet.message_hash();
                                let addr = packet.original_packet().meta().socket_addr();
                                let from_staked = packet.original_packet().meta().flags.contains(PacketFlags::FROM_STAKED_NODE);
                                let payer = packet.transaction().get_message().message.static_account_keys().get(0).unwrap();

                                info!("Source {addr}, txn_hash: {hash}, prio_per_unit: {prio_per_unit}, payer: {payer}, from_staked: {from_staked}");
                            }
                            None => {
                                stats.failed_priority += 1;
                            }
                        };
                    }
                    Err(_) => {
                        error!("Error decoding packet");
                        stats.failed_decoding += 1;
                    }
                };
            }
        }

        stats
    }
}