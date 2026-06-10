#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

pub mod compat;
pub mod utils;

use anyhow::Context;
use bdk_chain::bitcoin::{Address, Amount, BlockHash, ScriptPubKeyBuf, Txid};
use bdk_chain::CheckPoint;
use bitcoin::address::NetworkChecked;
use compat::bitcoin_032;
use core::time::Duration;
use electrsd::bitcoind::mtype::GetBlockTemplate;
use electrsd::bitcoind::{TemplateRequest, TemplateRules};

pub use electrsd;
pub use electrsd::bitcoind;
pub use electrsd::bitcoind::anyhow;
pub use electrsd::corepc_client;
pub use electrsd::electrum_client;
use electrsd::electrum_client::ElectrumApi;

/// Struct for running a regtest environment with a single `bitcoind` node with an `electrs`
/// instance connected to it.
pub struct TestEnv {
    pub bitcoind: electrsd::bitcoind::BitcoinD,
    pub electrsd: electrsd::ElectrsD,
}

/// Configuration parameters.
#[derive(Debug)]
pub struct Config<'a> {
    /// [`bitcoind::Conf`]
    pub bitcoind: bitcoind::Conf<'a>,
    /// [`electrsd::Conf`]
    pub electrsd: electrsd::Conf<'a>,
}

impl Default for Config<'_> {
    /// Use the default configuration plus set `http_enabled = true` for [`electrsd::Conf`]
    /// which is required for testing `bdk_esplora`.
    fn default() -> Self {
        Self {
            bitcoind: bitcoind::Conf::default(),
            electrsd: {
                let mut conf = electrsd::Conf::default();
                conf.http_enabled = true;
                conf
            },
        }
    }
}

/// Parameters for [`TestEnv::mine_block`].
#[non_exhaustive]
#[derive(Default)]
pub struct MineParams {
    /// If `true`, the block will be empty (no mempool transactions).
    pub empty: bool,
    /// Set a custom block timestamp. Defaults to `max(min_time, now)`.
    pub time: Option<u32>,
    /// Set a custom coinbase output script. Defaults to `OP_TRUE`.
    pub coinbase_address: Option<ScriptPubKeyBuf>,
}

impl MineParams {
    fn address_or_anyone_can_spend(&self) -> bitcoin_032::ScriptBuf {
        self.coinbase_address
            .clone()
            .map(compat::spk_to_032)
            // OP_TRUE (anyone can spend)
            .unwrap_or_else(|| {
                bitcoin_032::script::Builder::new()
                    .push_opcode(bitcoin_032::opcodes::OP_TRUE)
                    .into_script()
            })
    }
}

impl TestEnv {
    /// Construct a new [`TestEnv`] instance with the default configuration used by BDK.
    pub fn new() -> anyhow::Result<Self> {
        TestEnv::new_with_config(Config::default())
    }

    /// Construct a new [`TestEnv`] instance with the provided [`Config`].
    pub fn new_with_config(config: Config) -> anyhow::Result<Self> {
        let bitcoind_exe = match std::env::var("BITCOIND_EXE") {
            Ok(path) => path,
            Err(_) => bitcoind::downloaded_exe_path().context(
                "you need to provide an env var BITCOIND_EXE or specify a bitcoind version feature",
            )?,
        };
        let bitcoind = bitcoind::BitcoinD::with_conf(bitcoind_exe, &config.bitcoind)?;

        let electrs_exe = match std::env::var("ELECTRS_EXE") {
            Ok(path) => path,
            Err(_) => electrsd::downloaded_exe_path()
                .context("electrs version feature must be enabled")?,
        };
        let electrsd = electrsd::ElectrsD::with_conf(electrs_exe, &bitcoind, &config.electrsd)?;

        Ok(Self { bitcoind, electrsd })
    }

    /// Exposes the [`ElectrumApi`] calls from the Electrum client.
    pub fn electrum_client(&self) -> &impl ElectrumApi {
        &self.electrsd.client
    }

    /// Exposes the RPC calls from [`corepc_client`].
    pub fn rpc_client(&self) -> &bitcoind::Client {
        &self.bitcoind.client
    }

    // Reset `electrsd` so that new blocks can be seen.
    pub fn reset_electrsd(mut self) -> anyhow::Result<Self> {
        let mut electrsd_conf = electrsd::Conf::default();
        electrsd_conf.http_enabled = true;
        let electrsd = match std::env::var_os("ELECTRS_EXE") {
            Some(env_electrs_exe) => {
                electrsd::ElectrsD::with_conf(env_electrs_exe, &self.bitcoind, &electrsd_conf)
            }
            None => {
                let electrs_exe = electrsd::downloaded_exe_path()
                    .expect("electrs version feature must be enabled");
                electrsd::ElectrsD::with_conf(electrs_exe, &self.bitcoind, &electrsd_conf)
            }
        }?;
        self.electrsd = electrsd;
        Ok(self)
    }

    /// Mine a number of blocks of a given size `count`, which may be specified to a given coinbase
    /// `address`.
    pub fn mine_blocks(
        &self,
        count: usize,
        address: Option<Address>,
    ) -> anyhow::Result<Vec<BlockHash>> {
        let coinbase_address = match address {
            Some(address) => compat::address_to_032(&address),
            None => self.bitcoind.client.new_address()?,
        };
        let block_hashes = self
            .bitcoind
            .client
            .generate_to_address(count as _, &coinbase_address)?
            .into_model()?
            .0
            .into_iter()
            .map(compat::blockhash_from_032)
            .collect();
        Ok(block_hashes)
    }

    /// Get a block template from the node.
    pub fn get_block_template(&self) -> anyhow::Result<GetBlockTemplate> {
        Ok(self
            .bitcoind
            .client
            .get_block_template(&TemplateRequest {
                rules: vec![
                    TemplateRules::Segwit,
                    TemplateRules::Taproot,
                    TemplateRules::Csv,
                ],
            })?
            .into_model()?)
    }

    /// Mine a block that is guaranteed to be empty even with transactions in the mempool.
    #[cfg(feature = "std")]
    pub fn mine_empty_block(&self) -> anyhow::Result<(usize, BlockHash)> {
        self.mine_block(MineParams {
            empty: true,
            ..Default::default()
        })
    }

    /// Mine a single block with the given [`MineParams`].
    ///
    /// The block is constructed with the `bitcoin 0.32` types used by the RPC client; only the
    /// returned [`BlockHash`] crosses back into bdk's `bitcoin` version.
    pub fn mine_block(&self, params: MineParams) -> anyhow::Result<(usize, BlockHash)> {
        use bitcoin_032::hashes::Hash as _;
        use bitcoin_032::hex::FromHex as _;

        let bt = self.get_block_template()?;

        // BIP34 requires the height to be the first item in coinbase scriptSig.
        // Bitcoin Core validates by checking if scriptSig STARTS with the expected
        // encoding (using minimal opcodes like OP_1 for height 1).
        // The scriptSig must also be 2-100 bytes total.
        let coinbase_scriptsig = {
            let mut builder = bitcoin_032::script::Builder::new().push_int(bt.height as i64);
            for v in bt.coinbase_aux.values() {
                let bytes = Vec::<u8>::from_hex(v).expect("must be valid hex");
                let bytes_buf =
                    bitcoin_032::script::PushBytesBuf::try_from(bytes).expect("must be valid bytes");
                builder = builder.push_slice(bytes_buf);
            }
            // Ensure scriptSig is at least 2 bytes (pad with OP_0 if needed)
            if builder.as_bytes().len() < 2 {
                builder = builder.push_opcode(bitcoin_032::opcodes::OP_0);
            }
            builder.into_script()
        };

        let coinbase_outputs = if params.empty {
            let tx_fees: bitcoin_032::Amount = bt
                .transactions
                .iter()
                .map(|tx| tx.fee.to_unsigned().expect("fee must be positive"))
                .sum();
            let value = bt
                .coinbase_value
                .to_unsigned()
                .expect("coinbase_value must be positive")
                - tx_fees;
            vec![bitcoin_032::TxOut {
                value,
                script_pubkey: params.address_or_anyone_can_spend(),
            }]
        } else {
            core::iter::once(bitcoin_032::TxOut {
                value: bt
                    .coinbase_value
                    .to_unsigned()
                    .expect("coinbase_value must be positive"),
                script_pubkey: params.address_or_anyone_can_spend(),
            })
            .chain(
                bt.default_witness_commitment
                    .as_ref()
                    .map(|s| -> Result<_, bitcoin_032::hex::HexToBytesError> {
                        Ok(bitcoin_032::TxOut {
                            value: bitcoin_032::Amount::ZERO,
                            script_pubkey: bitcoin_032::ScriptBuf::from_hex(s)?,
                        })
                    })
                    .transpose()?,
            )
            .collect()
        };

        let coinbase_tx = bitcoin_032::Transaction {
            version: bitcoin_032::transaction::Version::ONE,
            lock_time: bitcoin_032::absolute::LockTime::from_height(0)?,
            input: vec![bitcoin_032::TxIn {
                previous_output: bitcoin_032::OutPoint::default(),
                script_sig: coinbase_scriptsig,
                sequence: bitcoin_032::Sequence::default(),
                witness: bitcoin_032::Witness::new(),
            }],
            output: coinbase_outputs,
        };

        let txdata = if params.empty {
            vec![coinbase_tx]
        } else {
            core::iter::once(coinbase_tx)
                .chain(bt.transactions.iter().map(|tx| tx.data.clone()))
                .collect()
        };

        let mut block = bitcoin_032::Block {
            header: bitcoin_032::block::Header {
                version: bt.version,
                prev_blockhash: bt.previous_block_hash,
                merkle_root: bitcoin_032::TxMerkleNode::from_raw_hash(
                    bitcoin_032::merkle_tree::calculate_root(
                        txdata.iter().map(|tx| tx.compute_txid().to_raw_hash()),
                    )
                    .expect("must have atleast one tx"),
                ),
                time: params.time.unwrap_or(Ord::max(
                    bt.min_time,
                    std::time::UNIX_EPOCH.elapsed()?.as_secs() as u32,
                )),
                bits: bt.bits,
                nonce: 0,
            },
            txdata,
        };

        block.header.merkle_root = block.compute_merkle_root().expect("must compute");

        // Mine!
        let target = block.header.target();
        for nonce in 0..=u32::MAX {
            block.header.nonce = nonce;
            let blockhash = block.block_hash();
            if target.is_met_by(blockhash) {
                self.rpc_client().submit_block(&block)?;
                return Ok((bt.height as usize, compat::blockhash_from_032(blockhash)));
            }
        }

        Err(anyhow::anyhow!("Cannot find nonce that meets the target"))
    }

    /// This method waits for the Electrum notification indicating that a new block has been mined.
    /// `timeout` is the maximum [`Duration`] we want to wait for a response from Electrsd.
    pub fn wait_until_electrum_sees_block(&self, timeout: Duration) -> anyhow::Result<()> {
        self.electrsd.client.block_headers_subscribe()?;
        let delay = Duration::from_millis(200);
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            self.electrsd.trigger()?;
            self.electrsd.client.ping()?;
            if self.electrsd.client.block_headers_pop()?.is_some() {
                return Ok(());
            }

            std::thread::sleep(delay);
        }

        Err(anyhow::Error::msg(format!(
            "Timed out waiting for Electrsd to get transaction, took: {:?}",
            start.elapsed()
        )))
    }

    /// This method waits for Electrsd to see a transaction with given `txid`. `timeout` is the
    /// maximum [`Duration`] we want to wait for a response from Electrsd.
    pub fn wait_until_electrum_sees_txid(
        &self,
        txid: Txid,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        let delay = Duration::from_millis(200);
        let start = std::time::Instant::now();

        let txid = compat::txid_to_032(txid);
        while start.elapsed() < timeout {
            if self.electrsd.client.transaction_get(&txid).is_ok() {
                return Ok(());
            }

            std::thread::sleep(delay);
        }

        Err(anyhow::Error::msg(format!(
            "Timed out waiting for Electrsd to get transaction, took: {:?}",
            start.elapsed()
        )))
    }

    /// Invalidate a number of blocks of a given size `count`.
    pub fn invalidate_blocks(&self, count: usize) -> anyhow::Result<()> {
        let mut hash = self.bitcoind.client.get_best_block_hash()?.block_hash()?;
        for _ in 0..count {
            let prev_hash = self
                .bitcoind
                .client
                .get_block_verbose_one(hash)?
                .into_model()?
                .previous_block_hash;
            self.bitcoind.client.invalidate_block(hash)?;
            match prev_hash {
                Some(prev_hash) => hash = prev_hash,
                None => break,
            }
        }
        Ok(())
    }

    /// Reorg a number of blocks of a given size `count`.
    /// Refer to [`TestEnv::mine_empty_block`] for more information.
    pub fn reorg(&self, count: usize) -> anyhow::Result<Vec<BlockHash>> {
        let start_height = self.bitcoind.client.get_block_count()?;
        self.invalidate_blocks(count)?;

        let res = self.mine_blocks(count, None);
        assert_eq!(
            self.bitcoind.client.get_block_count()?,
            start_height,
            "reorg should not result in height change"
        );
        res
    }

    /// Reorg with a number of empty blocks of a given size `count`.
    #[cfg(feature = "std")]
    pub fn reorg_empty_blocks(&self, count: usize) -> anyhow::Result<Vec<(usize, BlockHash)>> {
        let start_height = self.bitcoind.client.get_block_count()?;
        self.invalidate_blocks(count)?;

        let res = (0..count)
            .map(|_| self.mine_empty_block())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            self.bitcoind.client.get_block_count()?,
            start_height,
            "reorg should not result in height change"
        );
        Ok(res)
    }

    /// Send a tx of a given `amount` to a given `address`.
    pub fn send(&self, address: &Address<NetworkChecked>, amount: Amount) -> anyhow::Result<Txid> {
        let txid = self
            .bitcoind
            .client
            .send_to_address(
                &compat::address_to_032(address),
                compat::amount_to_032(amount),
            )?
            .txid()?;
        Ok(compat::txid_from_032(txid))
    }

    /// Create a checkpoint linked list of all the blocks in the chain.
    pub fn make_checkpoint_tip(&self) -> CheckPoint<BlockHash> {
        CheckPoint::from_blocks((0_u32..).map_while(|height| {
            self.get_block_hash(height as u64)
                .ok()
                .map(|block_hash| (height, block_hash))
        }))
        .expect("must craft tip")
    }

    /// Get the genesis hash of the blockchain.
    pub fn genesis_hash(&self) -> anyhow::Result<BlockHash> {
        let hash = self.bitcoind.client.get_block_hash(0)?.into_model()?.0;
        Ok(compat::blockhash_from_032(hash))
    }

    /// Get block hash by `height` from the `bitcoind` client.
    pub fn get_block_hash(&self, height: u64) -> anyhow::Result<BlockHash> {
        Ok(compat::blockhash_from_032(
            self.bitcoind.client.get_block_hash(height)?.block_hash()?,
        ))
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod test {
    use crate::compat::{self, bitcoin_032};
    use crate::{MineParams, TestEnv};
    use bdk_chain::bitcoin::Amount;
    use core::time::Duration;
    use electrsd::bitcoind::anyhow::Result;
    use std::collections::BTreeSet;

    /// This checks that reorgs initiated by `bitcoind` is detected by our `electrsd` instance.
    #[test]
    fn test_reorg_is_detected_in_electrsd() -> Result<()> {
        let env = TestEnv::new()?;

        // Mine some blocks.
        env.mine_blocks(101, None)?;
        env.wait_until_electrum_sees_block(Duration::from_secs(6))?;
        let height = env.bitcoind.client.get_block_count()?.into_model().0;
        let blocks = (0..=height)
            .map(|i| env.bitcoind.client.get_block_hash(i))
            .collect::<Result<Vec<_>, _>>()?;

        // Perform reorg on six blocks.
        env.reorg(6)?;
        env.wait_until_electrum_sees_block(Duration::from_secs(6))?;
        let reorged_height = env.bitcoind.client.get_block_count()?.into_model().0;
        let reorged_blocks = (0..=height)
            .map(|i| env.bitcoind.client.get_block_hash(i))
            .collect::<Result<Vec<_>, _>>()?;

        assert_eq!(height, reorged_height);

        // Block hashes should not be equal on the six reorged blocks.
        for (i, (block, reorged_block)) in blocks.iter().zip(reorged_blocks.iter()).enumerate() {
            match i <= height as usize - 6 {
                true => assert_eq!(block, reorged_block),
                false => assert_ne!(block, reorged_block),
            }
        }

        Ok(())
    }

    #[test]
    fn test_mine_block() -> Result<()> {
        let anyone_can_spend = bitcoin_032::script::Builder::new()
            .push_opcode(bitcoin_032::opcodes::OP_TRUE)
            .into_script();

        let env = TestEnv::new()?;

        // So we can spend.
        let addr = compat::address_from_032(
            &env.rpc_client()
                .get_new_address(None, None)?
                .address()?
                .assume_checked(),
        );
        env.mine_blocks(100, Some(addr.clone()))?;

        // Try mining a block with custom time.
        let custom_time = env.get_block_template()?.min_time + 100;
        let (_a_height, a_hash) = env.mine_block(MineParams {
            empty: false,
            time: Some(custom_time),
            coinbase_address: None,
        })?;
        let a_block = env
            .rpc_client()
            .get_block(compat::blockhash_to_032(a_hash))?;
        assert_eq!(a_block.header.time, custom_time);
        assert_eq!(
            a_block.txdata[0].output[0].script_pubkey, anyone_can_spend,
            "Subsidy address must be anyone_can_spend"
        );

        // Now try mining with min time & some txs.
        let txid1 = env.send(&addr, Amount::from_sat_u32(100_000))?;
        let txid2 = env.send(&addr, Amount::from_sat_u32(200_000))?;
        let txid3 = env.send(&addr, Amount::from_sat_u32(300_000))?;
        let min_time = env.get_block_template()?.min_time;
        let (_b_height, b_hash) = env.mine_block(MineParams {
            empty: false,
            time: Some(min_time),
            coinbase_address: None,
        })?;
        let b_block = env
            .rpc_client()
            .get_block(compat::blockhash_to_032(b_hash))?;
        assert_eq!(b_block.header.time, min_time);
        assert_eq!(
            a_block.txdata[0].output[0].script_pubkey, anyone_can_spend,
            "Subsidy address must be anyone_can_spend"
        );
        assert_eq!(
            b_block
                .txdata
                .iter()
                .skip(1) // ignore coinbase
                .map(|tx| compat::txid_from_032(tx.compute_txid()))
                .collect::<BTreeSet<_>>(),
            [txid1, txid2, txid3].into_iter().collect(),
            "Must have all txs"
        );

        // Custom subsidy address.
        let (_c_height, c_hash) = env.mine_block(MineParams {
            empty: false,
            time: None,
            coinbase_address: Some(addr.script_pubkey()),
        })?;
        let c_block = env
            .rpc_client()
            .get_block(compat::blockhash_to_032(c_hash))?;
        assert_eq!(
            c_block.txdata[0].output[0].script_pubkey,
            compat::spk_to_032(addr.script_pubkey()),
            "Custom address works"
        );

        Ok(())
    }
}
