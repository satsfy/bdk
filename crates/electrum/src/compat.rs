//! Conversions between the `bitcoin` version used by bdk and the `bitcoin 0.32` used by
//! `electrum-client`.
//!
//! Cargo treats the two `bitcoin` versions as distinct crates, so identical-looking types do not
//! unify. Hashes are converted by copying the inner byte array and transactions/headers by
//! round-tripping their consensus encoding (the wire format is identical across versions).

/// The `bitcoin 0.32` crate as used by `electrum-client`.
pub use electrum_client::bitcoin as bitcoin_032;

use bdk_core::bitcoin::{block::Header, consensus, BlockHash, ScriptPubKey, Transaction, Txid};
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

/// Converts a [`bitcoin_032::block::Header`] to the `bitcoin` version used by bdk.
pub fn header_from_032(header: bitcoin_032::block::Header) -> Header {
    let bytes = bitcoin_032::consensus::encode::serialize(&header);
    consensus::encode::deserialize(&bytes).expect("header consensus encoding must round-trip")
}

/// Converts a `TxMerkleNode` to `bitcoin 0.32`.
pub fn merkle_node_to_032(
    node: bdk_core::bitcoin::TxMerkleNode,
) -> bitcoin_032::TxMerkleNode {
    bitcoin_032::TxMerkleNode::from_byte_array(node.to_byte_array())
}

/// Borrows a [`ScriptPubKey`] as a `bitcoin 0.32` script.
pub fn spk_to_032(spk: &ScriptPubKey) -> &bitcoin_032::Script {
    bitcoin_032::Script::from_bytes(spk.as_bytes())
}

/// An entry of a script history response, with the txid converted to the `bitcoin` version used
/// by bdk.
pub struct HistoryRes {
    /// Txid of the transaction.
    pub tx_hash: Txid,
    /// Confirmation height. Heights 0 and -1 are reserved for unconfirmed transactions.
    pub height: i32,
}

/// Converts a script history response from `electrum-client`.
pub fn history_from_032(history: Vec<electrum_client::GetHistoryRes>) -> Vec<HistoryRes> {
    history
        .into_iter()
        .map(|res| HistoryRes {
            tx_hash: txid_from_032(res.tx_hash),
            height: res.height,
        })
        .collect()
}
