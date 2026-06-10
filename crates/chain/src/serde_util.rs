//! Serde implementations for `bitcoin` types that no longer ship their own.
//!
//! Newer `bitcoin` versions removed the blanket `serde` derives from `Transaction`, `TxOut` and
//! `OutPoint`. These modules restore (de)serialization for the collection types used in
//! changesets, delegating per-element work to [`bitcoin::serde_as_consensus`].

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;
use core::str::FromStr;

use bitcoin::{OutPoint, Transaction, TxOut};
use serde::{
    de::{DeserializeSeed, MapAccess, SeqAccess, Visitor},
    Deserializer, Serialize, Serializer,
};

/// (De)serializes `BTreeSet<Arc<Transaction>>` as a sequence of consensus encodings.
pub mod tx_set {
    use super::*;

    struct TxRef<'a>(&'a Transaction);

    impl Serialize for TxRef<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            bitcoin::serde_as_consensus::serialize(self.0, s)
        }
    }

    pub fn serialize<S: Serializer>(
        txs: &BTreeSet<Arc<Transaction>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        s.collect_seq(txs.iter().map(|tx| TxRef(tx.as_ref())))
    }

    struct TxSeed;

    impl<'de> DeserializeSeed<'de> for TxSeed {
        type Value = Arc<Transaction>;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            bitcoin::serde_as_consensus::deserialize::<Transaction, D>(d).map(Arc::new)
        }
    }

    struct SetVisitor;

    impl<'de> Visitor<'de> for SetVisitor {
        type Value = BTreeSet<Arc<Transaction>>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "a sequence of consensus-encoded transactions")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut set = BTreeSet::new();
            while let Some(tx) = seq.next_element_seed(TxSeed)? {
                set.insert(tx);
            }
            Ok(set)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<BTreeSet<Arc<Transaction>>, D::Error> {
        d.deserialize_seq(SetVisitor)
    }
}

/// (De)serializes `BTreeMap<OutPoint, TxOut>` with `txid:vout` string keys and consensus-encoded
/// values.
pub mod txout_map {
    use super::*;

    struct TxOutRef<'a>(&'a TxOut);

    impl Serialize for TxOutRef<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            bitcoin::serde_as_consensus::serialize(self.0, s)
        }
    }

    pub fn serialize<S: Serializer>(
        map: &BTreeMap<OutPoint, TxOut>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        use alloc::string::ToString;
        s.collect_map(map.iter().map(|(op, txout)| (op.to_string(), TxOutRef(txout))))
    }

    struct TxOutSeed;

    impl<'de> DeserializeSeed<'de> for TxOutSeed {
        type Value = TxOut;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            bitcoin::serde_as_consensus::deserialize::<TxOut, D>(d)
        }
    }

    struct MapVisitor;

    impl<'de> Visitor<'de> for MapVisitor {
        type Value = BTreeMap<OutPoint, TxOut>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "a map of outpoints to consensus-encoded txouts")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut map = BTreeMap::new();
            while let Some(key) = access.next_key::<String>()? {
                let op = OutPoint::from_str(&key).map_err(serde::de::Error::custom)?;
                let txout = access.next_value_seed(TxOutSeed)?;
                map.insert(op, txout);
            }
            Ok(map)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<BTreeMap<OutPoint, TxOut>, D::Error> {
        d.deserialize_map(MapVisitor)
    }
}
