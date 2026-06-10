//! Conversions between the `bitcoin` version used by bdk and the `bitcoin 0.32` used by
//! `bitcoincore-rpc`.
//!
//! Cargo treats the two `bitcoin` versions as distinct crates, so identical-looking types do not
//! unify. Hashes are converted by copying the inner byte array and transactions/blocks by
//! round-tripping their consensus encoding (the wire format is identical across versions).

/// The `bitcoin 0.32` crate as used by `bitcoincore-rpc`.
pub use bitcoincore_rpc::bitcoin as bitcoin_032;

use bdk_core::bitcoin::{block::Checked, consensus, Block, BlockHash, Transaction, Txid};
use bitcoin_032::hashes::Hash as _;

/// Converts a [`bitcoin_032::Txid`] to the `bitcoin` version used by bdk.
pub fn txid_from_032(txid: bitcoin_032::Txid) -> Txid {
    Txid::from_byte_array(txid.to_byte_array())
}

/// Converts a [`Txid`] to `bitcoin 0.32`.
pub fn txid_to_032(txid: Txid) -> bitcoin_032::Txid {
    bitcoin_032::Txid::from_byte_array(txid.to_byte_array())
}

/// Converts a [`bitcoin_032::BlockHash`] to the `bitcoin` version used by bdk.
pub fn blockhash_from_032(hash: bitcoin_032::BlockHash) -> BlockHash {
    BlockHash::from_byte_array(hash.to_byte_array())
}

/// Converts a [`BlockHash`] to `bitcoin 0.32`.
pub fn blockhash_to_032(hash: BlockHash) -> bitcoin_032::BlockHash {
    bitcoin_032::BlockHash::from_byte_array(hash.to_byte_array())
}

/// Converts a [`bitcoin_032::Transaction`] to the `bitcoin` version used by bdk.
pub fn tx_from_032(tx: &bitcoin_032::Transaction) -> Transaction {
    let bytes = bitcoin_032::consensus::encode::serialize(tx);
    consensus::encode::deserialize(&bytes).expect("tx consensus encoding must round-trip")
}

/// Converts a [`Transaction`] to `bitcoin 0.32`.
pub fn tx_to_032(tx: &Transaction) -> bitcoin_032::Transaction {
    let bytes = consensus::encode::serialize(tx);
    bitcoin_032::consensus::encode::deserialize(&bytes)
        .expect("tx consensus encoding must round-trip")
}

/// Converts a [`bitcoin_032::Block`] to the `bitcoin` version used by bdk.
///
/// The block is assumed checked as it originates from a trusted chain source.
pub fn block_from_032(block: &bitcoin_032::Block) -> Block<Checked> {
    let bytes = bitcoin_032::consensus::encode::serialize(block);
    consensus::encode::deserialize::<Block>(&bytes)
        .expect("block consensus encoding must round-trip")
        .assume_checked(None)
}
