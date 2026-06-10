# bdk migration to rust-bitcoin master (0.33.0-beta): error census and fix report

Migration of the whole bdk workspace (7 crates) from `bitcoin 0.32` to rust-bitcoin git master
(`bitcoin 0.33.0-beta`, commit `578d72f`, post-beta with `bitcoin_hashes 1.0.0`).

End state: `cargo build --workspace --all-features --all-targets` passes, all 20 doctests pass,
and every test suite passes including the integration tests run against live regtest
`bitcoind`/`electrs`/`esplora` instances (chain 81, bitcoind_rpc 14, electrum 12, esplora 14,
core 40+, testenv 2, file_store all).

## Methodology

Errors were collected with `cargo check --message-format=json` per crate, per fix round, into
27 log files (`/tmp/bdk-mig-logs/*.tsv`). Raw error instances observed across all rounds: **1135**;
unique error sites (file + code + message): **704** (of which ~93 were transient, self-inflicted
by an over-broad sed and excluded from analysis below).

The compiler numbers *understate* the real breakage: several large categories were fixed
mechanically by scripted rewrites before the compiler ever saw them. Measured from the final diff
(52 files, +1592/-1267 lines):

| silently-fixed category | sites |
|---|---|
| `Amount::from_sat(lit)` -> `from_sat_u32(lit)` (now fallible) | 132 |
| `ScriptBuf` -> `ScriptPubKeyBuf` (role-typed scripts) | 176 |
| `TxOut { value: ... }` -> `TxOut { amount: ... }` | 101 |
| `Transaction { input/output }` -> `{ inputs/outputs }` literals + accesses | ~190 |
| `OutPoint::new(a, b)` -> struct literal | 71 |
| `TxIn::default()` / `..Default::default()` -> `TxIn::EMPTY_COINBASE` | 60 |
| `*::all_zeros()` -> `from_byte_array([0; 32])` | 29+ |
| explicit `.expect(...)` added for newly-fallible ops | ~100 |
| cross-version conversion calls inserted (`compat::*`) | 230 |

## Error categories by frequency (unique compiler-error sites)

| # | category | sites | error codes |
|---|---|---|---|
| 1 | removed constructors/constants (`OutPoint::new/null/default`, `TxIn: Default`, `TxOut::NULL`, `Sequence` path + `ENABLE_RBF_NO_LOCKTIME` rename, `Version::non_standard`, `CompactTarget::default`, opcodes `OP_TRUE/OP_FALSE/OP_0` privatized into `all::`, `new_p2tr` signature, `from_slice` removals, `Network::Testnet4` variant) | 133 | E0599, E0603, E0433, E0061 |
| 2 | two-bitcoin-versions type clashes (bdk on 0.33 vs miniscript/electrum-client/esplora-client/bitcoincore-rpc/corepc on 0.32) | 53 + most of the 298 E0308s | E0308, E0277, E0271 |
| 3 | tx fields pluralized: `tx.input`/`tx.output` -> `tx.inputs`/`tx.outputs` | 34 visible (+~190 silent) | E0609, E0560 |
| 4 | hashes 1.0: `Hash::hash` trait method removed (now inherent per type), `all_zeros` removed, `from_raw_hash`/`to_raw_hash` removed, `hash_newtype!` no longer derives Debug/Display/FromStr/serde, `Txid::LEN` gone | 31 | E0599, E0782, E0576 |
| 5 | serde impls removed from `Transaction`, `TxOut`, `OutPoint`, `Amount` | 22 | E0277 |
| 6 | hex API: `bitcoin::hashes::hex::FromHex` gone, `Vec::from_hex` -> `hex::decode_to_vec`, `ScriptBuf::from_hex` behind `ScriptBufExt` trait | 22 | E0432, E0599 |
| 7 | `TxOut.value` -> `TxOut.amount` | 19 visible (+101 silent) | E0609, E0560 |
| 8 | Block redesign: `Block<Checked>`/`Block<Unchecked>` type-state, `txdata`/`header` fields privatized behind `transactions()`/`header()` (Checked-only), `compute_merkle_root` gone from value side | 13 | E0609, E0616, E0599 |
| 9 | role-typed scripts: `Script`/`ScriptBuf` removed from root, replaced by `ScriptPubKey(Buf)`, `ScriptSig(Buf)`, `RedeemScript`, `WitnessScript`, `TapScript` + `*Ext` extension traits carrying former inherent methods | 9 visible (+176 silent) | E0432, E0433 |
| 10 | secp256k1 0.32-beta: global-context API (`SecretKey::public_key()` takes no ctx, `new_p2tr(key, merkle_root)` drops ctx), `bitcoin::key::Secp256k1` path gone | 8 | E0432, E0061, E0277 |
| 11 | amount arithmetic: `Add/Sub/Mul` return `NumOpResult<T>` (must-use), `AddAssign`/`Sum<Amount>` gone, `from_sat` fallible (MAX_MONEY invariant), `to_signed()` now infallible | 6 visible (+~230 silent) | E0277, E0308, E0599 |
| 12 | feature rename: `bitcoin/rand-std` removed (now `rand`) — blocked **dependency resolution** before any code compiled | 1 | cargo resolver |

## The two hardest problems (not visible in per-site counts)

### 1. The ecosystem is still on bitcoin 0.32

`miniscript 13` (including its master branch), `bitcoincore-rpc 0.19`, `electrum-client 0.24`,
`esplora-client 0.12` and `corepc`/`electrsd` all pin `bitcoin 0.32`. Cargo resolves the two
majors as **distinct crates**, so `bitcoin::Txid` from one is a different type from the other,
and the lockfile permanently carries both trees (`bitcoin 0.32.8` + `0.33.0-beta`, two
`secp256k1`s, two `hex-conservative`s).

Fix strategy: per-crate `compat` conversion modules that cross the boundary by:
- copying the 32-byte array for hash types (`Txid`, `BlockHash`, `TxMerkleNode`),
- round-tripping the consensus encoding for `Transaction`/`Header`/`Block` (wire format is
  version-stable, so this is correctness-preserving),
- byte-moves for scripts (free for borrowed scripts: `Script::from_bytes(spk.as_bytes())`),
- string round-trips for addresses, sat round-trips for amounts.

New modules: `bdk_chain::compat` (miniscript boundary), `bdk_testenv::compat` (corepc/electrum,
re-exported for all test suites), `bdk_electrum::compat`, `bdk_esplora::compat` (needed an
explicit renamed `bitcoin_032` dependency because esplora-client re-exports types but not the
crate), `bdk_bitcoind_rpc::compat`. The descriptor pipeline in `bdk_chain`
(`SpkIterator`, `KeychainTxOutIndex`, `DescriptorExt`) derives spks in the 0.32 world and converts
each spk once at derivation time.

`bdk_testenv` now builds blocks to mine entirely in the 0.32 world (matching the RPC client) and
only converts the resulting block hash back to bdk types.

### 2. serde removed from core types breaks ChangeSet persistence

`Transaction`, `TxOut` and `OutPoint` no longer implement `Serialize`/`Deserialize`;
`Amount` serde also went away. bdk's `tx_graph::ChangeSet` derives serde over
`BTreeSet<Arc<Transaction>>` and `BTreeMap<OutPoint, TxOut>`, and `Balance` over four `Amount`s.

Fix: new `bdk_chain::serde_util` with `#[serde(with = ...)]` modules that delegate per element to
the new opt-in `bitcoin::serde_as_consensus` (hex string in human-readable formats, raw bytes
otherwise), with `OutPoint` map keys as `"txid:vout"` strings via Display/FromStr; `Balance`
fields use `bitcoin::amount::serde::as_sat` (wire-compatible with the old u64-sats encoding).

**Behavioral break to flag**: the serialized format of `tx_graph::ChangeSet::txs` and `::txouts`
changes (structural JSON -> consensus hex / `"txid:vout"` keys). Old persisted JSON/bincode data
will not deserialize. `Balance` and the sqlite store keep their formats (sqlite already stored
consensus blobs and sat integers; schema and column names unchanged).

## Other design decisions taken

- **Checked blocks at the bdk boundary**: `IndexedTxGraph::apply_block(_relevant)`,
  `TxPosInBlock.block` and the RPC `Emitter`/`FilterIter` now use `Block<Checked>`. Blocks come
  from trusted chain sources; conversion happens via `assume_checked(None)` inside the compat
  layer (bdk never validated blocks itself before either).
- **Role-typed scripts**: everything bdk indexes is a script pubkey, so `ScriptBuf` became
  `ScriptPubKeyBuf` throughout the public API (not a type alias) and `script_sig` fields use
  `ScriptSigBuf`.
- **Overflow handling**: balance/fee accumulation uses the new checked ops with
  `.expect("...must not overflow")`, matching the previous panic-on-overflow semantics of 0.32's
  `Add` impls (but now with explicit messages).
- **`DescriptorId`**: restored `Debug`/`Display`/`FromStr`/serde via the new hashes-1.0 companion
  macros `impl_hex_for_newtype!` / `impl_serde_for_newtype!` (encodings unchanged).
- **Test placeholders**: `TxOut::NULL` (deleted upstream, was `u64::MAX` sats) replaced by a
  zero-amount/empty-spk placeholder; `hash!`/`block_id!` testenv macros rebuilt on a small
  `TestHash` trait (sha256d, same digests as before).

## Notable upstream changes that did NOT break bdk (for completeness)

Psbt serde format change, locktime serde change and `PartialOrd` removal, `VarInt` removal
(consensus-encoding rewrite), `Weight`/`FeeRate` const-ification, taproot error splits —
bdk does not touch these surfaces directly.

## Verification

- `cargo build --workspace --all-features --all-targets` — clean (warnings only).
- `cargo test --workspace --all-features --doc` — 20/20 pass.
- `cargo test -p <crate> --all-features` for all 7 crates — all pass, including regtest
  integration tests (bitcoind + electrs + esplora spun up by testenv).

## Blockers for actually landing this

This compiles and passes tests, but it should not ship as-is:

1. rust-bitcoin master is explicitly a moving beta (`0.33.0` will never be released; next is
   `0.34.0-beta`). Pinning `branch = "master"` means every upstream merge can break the build.
2. The dual-bitcoin-tree with byte/consensus shims is a workaround, not an architecture. The
   real unlock is miniscript + the client crates releasing 0.33-compatible versions, at which
   point all five `compat` modules and the renamed `bitcoin_032` dep can be deleted.
3. The ChangeSet serde format break needs a migration story for persisted wallets.
