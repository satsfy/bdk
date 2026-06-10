//! Conversions between types of `miniscript`'s `bitcoin` dependency and the `bitcoin` version
//! used by this crate.
//!
//! `miniscript` is still based on `bitcoin 0.32` while this crate tracks a newer `bitcoin`.
//! Cargo treats the two versions as distinct crates, so identical-looking types do not unify.
//! Values crossing the descriptor boundary are converted by round-tripping raw bytes/satoshis.

use bitcoin::{Amount, ScriptPubKeyBuf};

/// The `bitcoin` crate version used by `miniscript`.
pub use miniscript::bitcoin as ms_bitcoin;

/// Converts a script pubkey from miniscript's `bitcoin` version to ours.
pub fn spk_from_ms(spk: ms_bitcoin::ScriptBuf) -> ScriptPubKeyBuf {
    ScriptPubKeyBuf::from_bytes(spk.into_bytes())
}

/// Converts a script pubkey from our `bitcoin` version to miniscript's.
pub fn spk_to_ms(spk: ScriptPubKeyBuf) -> ms_bitcoin::ScriptBuf {
    ms_bitcoin::ScriptBuf::from_bytes(spk.into_bytes())
}

/// Converts an amount from miniscript's `bitcoin` version to ours.
///
/// # Panics
///
/// Panics if `amount` exceeds the total supply cap, an invariant the newer `bitcoin` enforces.
pub fn amount_from_ms(amount: ms_bitcoin::Amount) -> Amount {
    Amount::from_sat(amount.to_sat()).expect("amount must not exceed total supply")
}
