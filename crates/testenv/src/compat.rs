//! Conversions between the `bitcoin` version used by bdk and the `bitcoin 0.32` used by the
//! client crates (`corepc`, `electrum-client`).
//!
//! Cargo treats the two `bitcoin` versions as distinct crates, so identical-looking types do not
//! unify. Hashes are converted by copying the inner byte array, transactions/headers/blocks by
//! round-tripping their consensus encoding (the wire format is identical across versions) and
//! addresses via their string representation.

/// The `bitcoin 0.32` crate as used by the client libraries.
pub use electrsd::corepc_client::bitcoin as bitcoin_032;

use bdk_chain::bitcoin::{
    self, block::Checked, consensus, Address, Amount, Block, BlockHash, ScriptPubKeyBuf,
    Transaction, Txid,
};
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

/// Converts a [`bitcoin_032::block::Header`] to the `bitcoin` version used by bdk.
pub fn header_from_032(header: bitcoin_032::block::Header) -> bitcoin::block::Header {
    let bytes = bitcoin_032::consensus::encode::serialize(&header);
    consensus::encode::deserialize(&bytes).expect("header consensus encoding must round-trip")
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

/// Converts a script pubkey from `bitcoin 0.32`.
pub fn spk_from_032(spk: bitcoin_032::ScriptBuf) -> ScriptPubKeyBuf {
    ScriptPubKeyBuf::from_bytes(spk.into_bytes())
}

/// Converts a script pubkey to `bitcoin 0.32`.
pub fn spk_to_032(spk: ScriptPubKeyBuf) -> bitcoin_032::ScriptBuf {
    bitcoin_032::ScriptBuf::from_bytes(spk.into_bytes())
}

/// Converts a [`bitcoin_032::Amount`] to the `bitcoin` version used by bdk.
pub fn amount_from_032(amount: bitcoin_032::Amount) -> Amount {
    Amount::from_sat(amount.to_sat()).expect("amount must not exceed total supply")
}

/// Converts an [`Amount`] to `bitcoin 0.32`.
pub fn amount_to_032(amount: Amount) -> bitcoin_032::Amount {
    bitcoin_032::Amount::from_sat(amount.to_sat())
}

/// Converts a [`bitcoin_032::Address`] to the `bitcoin` version used by bdk.
pub fn address_from_032(address: &bitcoin_032::Address) -> Address {
    address
        .to_string()
        .parse::<Address<_>>()
        .expect("address representation must round-trip")
        .assume_checked()
}

/// Converts an [`Address`] to `bitcoin 0.32`.
pub fn address_to_032(address: &Address) -> bitcoin_032::Address {
    address
        .to_string()
        .parse::<bitcoin_032::Address<_>>()
        .expect("address representation must round-trip")
        .assume_checked()
}
